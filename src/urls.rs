//! Constructs URLs and redirects using an application's public address.
//!
//! An application may listen on a private socket but be publicly available at
//! `https://example.com/app/`. [`Urls`] uses that public address to turn
//! `account` into `/app/account` with [`Urls::internal`], or
//! `https://example.com/app/account` with [`Urls::external`].
//!
//! # Fixed public URL
//!
//! Prefer supplying the address in configuration:
//!
//! ```toml
//! public_url = "https://example.com/app/"
//! ```
//!
//! [`Core::urls`](crate::config::Core::urls) constructs a URL generator from this
//! value. It works without an HTTP request, including in background jobs:
//!
//! ```
//! # fn example(config: twelve::config::Core) {
//! let urls = config.urls().expect("public_url must be configured");
//! let url = urls.external("account").expect("valid path");
//! # }
//! ```
//!
//! Handlers can instead take `urls: Urls` as an Axum extractor. To supply its
//! configuration, add an [`axum::Extension`] layer to the router. This makes
//! the value available on each request:
//!
//! ```
//! use axum::{Extension, Router};
//!
//! # fn configure(app: Router, config: twelve::config::Core) -> Router {
//! let app = app.layer(Extension(config.public_url));
//! # app
//! # }
//! ```
//!
//! When `config.public_url` is `Some(url)`, the extractor uses that fixed URL
//! and ignores all origin and prefix headers. If it is `None`, this opts into
//! the request-derived behavior below.
//!
//! **Warning:** Without an `Extension<Option<PublicUrl>>`, extraction returns
//! HTTP 500. Loading configuration alone does not register it.
//!
//! # Request-derived URLs
//!
//! Registering `None::<PublicUrl>` instead of a fixed URL enables automatic
//! detection from request headers:
//!
//! ```
//! # use axum::{Extension, Router};
//! # use twelve::config::PublicUrl;
//! # let app: Router = Router::new();
//! let app = app.layer(Extension(None::<PublicUrl>));
//! ```
//!
//! Detection uses `X-Forwarded-Host` (fallback: `Host`), `X-Forwarded-Proto`
//! (default: HTTP), and `X-Script-Name` (default: no path prefix).
//!
//! **Warning:** Request-derived addresses can be attacker-controlled. For
//! example, an attacker requests a password reset for another user, supplying
//! their own domain in `X-Forwarded-Host` or `Host`. If the reset email uses
//! [`Urls::external`], its link points to the attacker. Clicking it sends them
//! the token, which they can use to reset the victim's password.
//!
//! To use detection safely, have the reverse proxy fix or validate the public
//! host and scheme, and set or remove `X-Script-Name`. Prevent direct access to
//! the backend so clients cannot bypass the proxy. A configured public URL
//! avoids relying on request headers.

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
    /// Indicates that the supplied path or prefix is not a valid URI.
    #[error("invalid public URL")]
    InvalidUri(#[source] axum::http::Error),
}

/// Constructs URLs from a fixed public address or request headers.
#[derive(Debug)]
pub struct Urls {
    /// The absolute path on the domain that the app is running under.
    script_name: Option<String>,
    /// The configured or request-derived HTTP scheme.
    scheme: Option<Scheme>,
    /// The public authority, including any credentials and port.
    authority: Option<Authority>,
}

impl Urls {
    /// Constructs a relative URL, joining the path prefix with one slash.
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

    /// Constructs an absolute URL relative to the base URL, not the request path.
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

    /// Redirects to a path relative to the configured or detected prefix.
    ///
    /// # Panics
    ///
    /// Panics if [`Urls::internal`] cannot construct a valid URI.
    #[inline(always)]
    pub fn redirect_to(&self, path: &str) -> Redirect {
        Redirect::to(&self.internal(path))
    }
}

impl From<&PublicUrl> for Urls {
    /// Uses a fixed public URL, including its credentials and path.
    fn from(url: &PublicUrl) -> Self {
        let uri = url.as_uri();
        Self {
            script_name: Some(uri.path().to_owned()),
            scheme: uri.scheme().cloned(),
            authority: uri.authority().cloned(),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Urls {
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

        Ok(Urls {
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

    use super::{ExternalUrlError, Urls};
    use crate::config::PublicUrl;

    #[test]
    fn internal_url_construction_without_reverse_proxy() {
        let urls = Urls {
            script_name: None,
            scheme: None,
            authority: None,
        };

        assert_eq!(urls.internal("/foo/bar"), "/foo/bar");
        assert!(matches!(
            urls.external("foo"),
            Err(ExternalUrlError::MissingOrigin)
        ));
    }

    #[test]
    fn internal_url_construction_with_reverse_proxy() {
        let urls = Urls {
            script_name: Some("/sub/dir///".to_owned()),
            scheme: None,
            authority: None,
        };

        assert_eq!(urls.internal("foo/bar"), "/sub/dir/foo/bar");
        assert_eq!(urls.internal("///foo/bar"), "/sub/dir/foo/bar");
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
        let urls = Urls::from_request_parts(&mut parts, &())
            .await
            .expect("configured URLs");
        assert_eq!(
            urls.external("account").expect("valid URL"),
            "https://user:p%40ss@example.com:8443/app/account"
        );
        assert_eq!(urls.internal("account"), "/app/account");
        assert_eq!(urls.public_host(), Some("example.com:8443"));
    }

    #[tokio::test]
    async fn header_based_urls_require_explicit_registration() {
        let (mut parts, ()) = Request::builder()
            .header("Host", "127.0.0.1:3000")
            .body(())
            .expect("valid request")
            .into_parts();
        assert_eq!(
            Urls::from_request_parts(&mut parts, &())
                .await
                .expect_err("missing registration"),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        parts.extensions.insert(None::<PublicUrl>);
        let urls = Urls::from_request_parts(&mut parts, &())
            .await
            .expect("request-derived URLs");
        assert_eq!(
            urls.external("foo").expect("valid URL"),
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
            let urls = Urls {
                script_name: prefix.map(str::to_owned),
                scheme: Some(Scheme::HTTPS),
                authority: Some("example.com:8443".parse().expect("valid authority")),
            };
            assert_eq!(urls.public_host(), Some("example.com:8443"));
            for path in ["foo?bar=baz", "/foo?bar=baz"] {
                assert_eq!(urls.external(path).expect("valid public URL"), expected);
            }
        }
    }
}
