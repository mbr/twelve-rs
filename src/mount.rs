//! Builds links and redirects for applications below a reverse-proxy path prefix.
//!
//! [`Mount`] reads `X-Script-Name` and prepends it to generated links and
//! redirects. [`Mount::external`] also reads `X-Forwarded-Proto` and
//! `X-Forwarded-Host` to construct absolute URLs. It does not rewrite routing.
//!
//! **Warning:** These headers are trusted unconditionally. Only use this
//! extractor behind a trusted reverse proxy that overwrites or removes all
//! three headers, and prevent clients from reaching the backend directly.
//! Remove `X-Script-Name` when no mount prefix is configured. Validating header
//! syntax does not prevent an attacker from supplying a different public URL.
//!
//! No fallback to `Forwarded`, `Host`, or the request URI is performed.
//!
//! ```
//! use axum::response::Redirect;
//! use twelve::mount::Mount;
//!
//! async fn account(mount: Mount) -> Redirect {
//!     mount.redirect_to("/account")
//! }
//! ```

use axum::{
    extract::FromRequestParts,
    http::{
        request::Parts,
        uri::{self, Authority, Scheme},
        HeaderMap, StatusCode, Uri,
    },
    response::Redirect,
};
use thiserror::Error;

/// Describes why an absolute public URL could not be constructed.
#[derive(Debug, Error)]
pub enum ExternalUrlError {
    /// Indicates that the proxy did not provide both origin headers.
    #[error("public URL requires X-Forwarded-Proto and X-Forwarded-Host")]
    MissingOrigin,
    /// Indicates that the supplied path or mount prefix is not a valid URI.
    #[error("invalid public URL")]
    InvalidUri(#[source] axum::http::Error),
}

/// Provides request-aware links for applications below a proxy path prefix.
#[derive(Debug)]
pub struct Mount {
    /// The absolute path on the domain that the app is running under.
    script_name: Option<String>,
    /// The public HTTP scheme supplied by the reverse proxy.
    scheme: Option<Scheme>,
    /// The public host and optional port supplied by the reverse proxy.
    authority: Option<Authority>,
}

impl Mount {
    /// Constructs a relative URL (no scheme or host).
    ///
    /// Exactly one slash separates the external mount prefix from the supplied
    /// path. Slashes elsewhere in either value are preserved.
    ///
    /// # Panics
    ///
    /// Will panic if generated Uris are invalid.
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

    /// Constructs an absolute URL using the proxy origin and mount prefix.
    ///
    /// Both `foo` and `/foo` are relative to the mount prefix, not the current
    /// request path. Returns an error if either origin header is absent or the
    /// resulting URI is invalid. The path must be URI-encoded.
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

    /// Returns the forwarded public host, including an explicit port if present.
    pub fn public_host(&self) -> Option<&str> {
        self.authority.as_ref().map(Authority::as_str)
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

impl<S: Send + Sync> FromRequestParts<S> for Mount {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
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

        let scheme = proxy_header(&parts.headers, "x-forwarded-proto")?
            .map(|value| match value {
                "http" => Ok(Scheme::HTTP),
                "https" => Ok(Scheme::HTTPS),
                _ => Err(StatusCode::BAD_GATEWAY),
            })
            .transpose()?;
        let authority = proxy_header(&parts.headers, "x-forwarded-host")?
            .map(|value| {
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

/// Reads a single proxy header, rejecting ambiguous repeated values.
fn proxy_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, StatusCode> {
    let mut values = headers.get_all(name).iter();
    let value = values.next();
    if values.next().is_some() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    value
        .map(|value| value.to_str().map_err(|_| StatusCode::BAD_GATEWAY))
        .transpose()
}

#[cfg(test)]
mod tests {
    use axum::http::uri::Scheme;

    use super::{ExternalUrlError, Mount};

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
