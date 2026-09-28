//! Builds relative links, absolute URLs, and redirects with a mount prefix.
//! Request routing is unchanged.
//!
//! Prefer a fixed public URL; request-header auto-detection is also available.
//! Set `public_url = "https://example.com/app/"` in configuration, then use
//! [`Core::mount`](crate::config::Core::mount), without a request or extension:
//!
//! ```
//! # fn example(config: twelve::config::Core) {
//! let mount = config.mount().expect("public_url must be configured");
//! let link = mount.external("account").expect("valid path");
//! // https://example.com/app/account
//! # }
//! ```
//!
//! For auto-detection, accept `mount: Mount` in a handler and explicitly opt in:
//!
//! ```
//! use axum::{Extension, Router};
//! use twelve::config::PublicUrl;
//!
//! # let app: Router = Router::new();
//! let app = app.layer(Extension(None::<PublicUrl>));
//! ```
//!
//! Registering `Some(url)` instead makes the extractor use that fixed URL.
//! **Warning:** Extraction requires `Extension<Option<PublicUrl>>`; missing
//! registration returns HTTP 500. Setting `Core::public_url` alone is not enough.
//!
//! Auto-detection uses `X-Forwarded-Proto` (default: HTTP), `X-Forwarded-Host`
//! (fallback: `Host`), and `X-Script-Name` (default: no prefix).
//!
//! **Warning:** These headers are trusted. For example, an attacker can request
//! a password reset for another user with their own domain in `X-Forwarded-Host`
//! or `Host`. A reset email built with [`Mount::external`] then links to that
//! domain. When the victim clicks, the attacker receives the reset token and
//! can use it to change the victim's password.
//!
//! For header-based URLs, the proxy must fix or validate the public origin and
//! set or remove `X-Script-Name`. Block direct backend access so clients cannot
//! bypass the proxy. Using a configured fixed URL avoids this dependency.

use axum::{
    extract::FromRequestParts,
    http::{
        request::Parts,
        uri::{self, Authority, Scheme},
        StatusCode, Uri,
    },
    response::Redirect,
};
use thiserror::Error;

use crate::config::PublicUrl;

/// Reports public URL construction failures.
#[derive(Debug, Error)]
pub enum ExternalUrlError {
    /// Indicates that no public origin is available.
    #[error("public URL requires a host")]
    MissingOrigin,
    /// Indicates that the supplied path or mount prefix is not a valid URI.
    #[error("invalid public URL")]
    InvalidUri(#[source] axum::http::Error),
}

/// Constructs links from a fixed public URL or request headers.
#[derive(Debug)]
pub struct Mount {
    /// The absolute path on the domain that the app is running under.
    script_name: Option<String>,
    /// The configured or request-derived HTTP scheme.
    scheme: Option<Scheme>,
    /// The public authority, including any credentials and port.
    authority: Option<Authority>,
}

impl Mount {
    /// Constructs a relative URL, joining the mount prefix with one slash.
    ///
    /// # Panics
    ///
    /// Panics if the resulting URI is invalid.
    // TODO: Log warning, generate different Uri, or ensure this can never fail?
    pub fn internal<S: AsRef<str>>(&self, path: S) -> String {
        let mut parts: uri::Parts = Default::default();

        if let Some(ref script_name) = self.script_name {
            let path = format!(
                "{}/{}",
                script_name.trim_end_matches('/'),
                path.as_ref().trim_start_matches('/'),
            );
            parts.path_and_query = Some(path.parse().expect("should not fail to parse"));
        } else {
            parts.path_and_query = Some(
                path.as_ref()
                    .parse()
                    .expect("tried to generate invalid Uri"),
            );
        }

        Uri::from_parts(parts)
            .expect("should not fail to construct relative uri")
            .to_string()
    }

    /// Constructs an absolute URL relative to the mount, not the request path.
    ///
    /// Accepts URI-encoded paths with or without a leading slash. Fails if the
    /// host is missing or the resulting URI is invalid.
    pub fn external<S: AsRef<str>>(&self, path: S) -> Result<String, ExternalUrlError> {
        let (scheme, authority) = self
            .scheme
            .as_ref()
            .zip(self.authority.as_ref())
            .ok_or(ExternalUrlError::MissingOrigin)?;
        let path = format!(
            "{}/{}",
            self.script_name
                .as_deref()
                .unwrap_or("")
                .trim_end_matches('/'),
            path.as_ref().trim_start_matches('/'),
        );
        Uri::builder()
            .scheme(scheme.clone())
            .authority(authority.clone())
            .path_and_query(path)
            .build()
            .map(|uri| uri.to_string())
            .map_err(ExternalUrlError::InvalidUri)
    }

    /// Returns the public host, including an explicit port if present.
    pub fn public_host(&self) -> Option<&str> {
        self.authority
            .as_ref()
            .and_then(|authority| authority.as_str().rsplit('@').next())
    }

    /// Redirects to a path relative to the mount prefix.
    ///
    /// # Panics
    ///
    /// Panics if [`Mount::internal`] cannot construct a valid URI.
    #[inline(always)]
    pub fn redirect_to(&self, path: &str) -> Redirect {
        Redirect::to(&self.internal(path))
    }
}

impl From<&PublicUrl> for Mount {
    /// Constructs a fixed mount, including the URL's credentials and path.
    fn from(url: &PublicUrl) -> Self {
        let uri = url.as_uri();
        Self {
            script_name: Some(uri.path().to_owned()),
            scheme: uri.scheme().cloned(),
            authority: uri.authority().cloned(),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Mount {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        if let Some(url) = parts
            .extensions
            .get::<Option<PublicUrl>>()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        {
            return Ok(Self::from(url));
        }

        let script_name = if let Some(script_name_header) = parts.headers.get("X-Script-Name") {
            Some(
                script_name_header
                    .to_str()
                    .map_err(|_| StatusCode::BAD_GATEWAY)?
                    .to_owned(),
            )
        } else {
            None
        };

        let scheme = parts
            .headers
            .get("X-Forwarded-Proto")
            .map(
                |value| match value.to_str().map_err(|_| StatusCode::BAD_GATEWAY)? {
                    "http" => Ok(Scheme::HTTP),
                    "https" => Ok(Scheme::HTTPS),
                    _ => Err(StatusCode::BAD_GATEWAY),
                },
            )
            .transpose()?
            .or(Some(Scheme::HTTP));
        let authority = parts
            .headers
            .get("X-Forwarded-Host")
            .or_else(|| parts.headers.get("Host"))
            .map(|value| {
                let value = value.to_str().map_err(|_| StatusCode::BAD_GATEWAY)?;
                if value.contains(['@', ',', '\\']) {
                    return Err(StatusCode::BAD_GATEWAY);
                }
                value
                    .parse::<Authority>()
                    .map_err(|_| StatusCode::BAD_GATEWAY)
            })
            .transpose()?;

        Ok(Mount {
            script_name,
            scheme,
            authority,
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::FromRequestParts,
        http::{uri::Scheme, Request, StatusCode},
    };

    use super::{ExternalUrlError, Mount};
    use crate::config::PublicUrl;

    #[test]
    fn internal_url_construction_without_reverse_proxy() {
        let mount = Mount {
            script_name: None,
            scheme: None,
            authority: None,
        };

        assert_eq!(mount.internal("/foo/bar"), "/foo/bar");
        assert!(matches!(
            mount.external("foo"),
            Err(ExternalUrlError::MissingOrigin)
        ));
    }

    #[test]
    fn internal_url_construction_with_reverse_proxy() {
        let mount = Mount {
            script_name: Some("/sub/dir///".to_owned()),
            scheme: None,
            authority: None,
        };

        assert_eq!(mount.internal("foo/bar"), "/sub/dir/foo/bar");
        assert_eq!(mount.internal("///foo/bar"), "/sub/dir/foo/bar");
    }

    #[tokio::test]
    async fn configured_url_overrides_request_headers() {
        let url: PublicUrl = "https://user:p%40ss@example.com:8443/app/"
            .parse()
            .expect("valid public URL");
        let (mut parts, ()) = Request::builder()
            .header("X-Forwarded-Proto", "invalid")
            .header("X-Forwarded-Host", "attacker.example")
            .header("Host", "attacker.example")
            .header("X-Script-Name", "/attacker")
            .body(())
            .expect("valid request")
            .into_parts();
        parts.extensions.insert(Some(url));
        let mount = Mount::from_request_parts(&mut parts, &())
            .await
            .expect("fixed mount");
        assert_eq!(
            mount.external("account").expect("valid URL"),
            "https://user:p%40ss@example.com:8443/app/account"
        );
        assert_eq!(mount.internal("account"), "/app/account");
        assert_eq!(mount.public_host(), Some("example.com:8443"));
    }

    #[tokio::test]
    async fn header_based_urls_require_explicit_registration() {
        let (mut parts, ()) = Request::builder()
            .header("Host", "127.0.0.1:3000")
            .body(())
            .expect("valid request")
            .into_parts();
        assert_eq!(
            Mount::from_request_parts(&mut parts, &())
                .await
                .expect_err("missing registration"),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        parts.extensions.insert(None::<PublicUrl>);
        let mount = Mount::from_request_parts(&mut parts, &())
            .await
            .expect("valid mount");
        assert_eq!(
            mount.external("foo").expect("valid URL"),
            "http://127.0.0.1:3000/foo"
        );
    }

    #[test]
    fn external_url_construction() {
        for (prefix, expected) in [
            (None, "https://example.com:8443/foo?bar=baz"),
            (
                Some("/script-path/bla///"),
                "https://example.com:8443/script-path/bla/foo?bar=baz",
            ),
        ] {
            let mount = Mount {
                script_name: prefix.map(str::to_owned),
                scheme: Some(Scheme::HTTPS),
                authority: Some("example.com:8443".parse().expect("valid authority")),
            };
            assert_eq!(mount.public_host(), Some("example.com:8443"));
            for path in ["foo?bar=baz", "/foo?bar=baz"] {
                assert_eq!(mount.external(path).expect("valid public URL"), expected);
            }
        }
    }
}
