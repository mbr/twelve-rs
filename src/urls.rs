//! Constructs URLs and redirects using an application's public address.
//!
//! An application may listen on a private socket but be publicly available at
//! `https://example.com/app/`. [`Urls`] uses that public address to turn
//! `account` into `/app/account` with [`Urls::internal`], or
//! `https://example.com/app/account` with [`Urls::external`].
//!
//! Prefer `internal()` for navigation: relative URLs retain the visitor's host
//! and port, including during development or when accessing the app by IP.
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
//! Handlers can instead take `urls: Urls` as an Axum extractor. Register a
//! [`Core::url_source`](crate::config::Core::url_source) using
//! [`axum::Extension`] to use the configured URL, falling back to automatic
//! detection when `public_url` is absent:
//!
//! ```
//! use axum::{Extension, Router};
//!
//! # fn configure(app: Router, config: twelve::config::Core) -> Router {
//! let app = app.layer(Extension(config.url_source()));
//! # app
//! # }
//! ```
//!
//! With a configured URL, this ignores origin and prefix headers. Otherwise,
//! the request-derived behavior and security considerations below apply.
//! **Warning:** Without `Extension<UrlSource>`, extraction returns HTTP 500.
//! Loading configuration alone does not register it.
//!
//! # Request-derived URLs
//!
//! Register [`UrlSource::Automatic`] to detect the address from request headers:
//!
//! ```
//! # use axum::{Extension, Router};
//! # use twelve::urls::UrlSource;
//! # let app: Router = Router::new();
//! let app = app.layer(Extension(UrlSource::Automatic));
//! ```
//!
//! Detection uses `X-Forwarded-Host` (fallback: `Host`), `X-Forwarded-Proto`
//! (default: HTTP), and `X-Script-Name` (default: `/`). A missing or invalid host
//! returns HTTP 400; successful extraction always provides a complete address.
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
        uri::{Authority, Scheme},
        StatusCode, Uri,
    },
    response::Redirect,
};
use thiserror::Error;

use crate::config::PublicUrl;

/// Selects how request extraction resolves the application's public address.
#[derive(Clone, Debug)]
pub enum UrlSource {
    /// Uses a fixed public URL without consulting request headers.
    Explicit(PublicUrl),
    /// Resolves the public address from request headers.
    Automatic,
}

/// Reports public URL construction failures.
#[derive(Debug, Error)]
pub enum ExternalUrlError {
    /// Indicates that the supplied path or prefix is not a valid URI.
    #[error("invalid public URL")]
    InvalidUri(#[source] axum::http::Error),
}

/// Constructs URLs from a resolved public address.
#[derive(Clone, Debug)]
pub struct Urls {
    /// The absolute path on the domain that the app is running under.
    script_name: String,
    /// The configured or request-derived HTTP scheme.
    scheme: Scheme,
    /// The public authority, including any credentials and port.
    authority: Authority,
}

impl Urls {
    /// Constructs a relative URL, joining the path prefix with one slash.
    ///
    /// # Panics
    ///
    /// Panics if the resulting URI is invalid.
    pub fn internal<S: AsRef<str>>(&self, path: S) -> String {
        Uri::builder()
            .path_and_query(self.prefixed_path(path.as_ref()))
            .build()
            .expect("invalid internal URI")
            .to_string()
    }

    /// Constructs an absolute URL relative to the base URL, not the request path.
    ///
    /// Accepts URI-encoded paths with or without a leading slash. Fails if the
    /// resulting URI is invalid.
    pub fn external<S: AsRef<str>>(&self, path: S) -> Result<String, ExternalUrlError> {
        Uri::builder()
            .scheme(self.scheme.clone())
            .authority(self.authority.clone())
            .path_and_query(self.prefixed_path(path.as_ref()))
            .build()
            .map(|uri| uri.to_string())
            .map_err(ExternalUrlError::InvalidUri)
    }

    /// Joins a path to the application's public prefix.
    fn prefixed_path(&self, path: &str) -> String {
        format!(
            "{}/{}",
            self.script_name.trim_end_matches('/'),
            path.trim_start_matches('/'),
        )
    }

    /// Returns the public host, including an explicit port if present.
    pub fn public_host(&self) -> &str {
        let authority = self.authority.as_str();
        authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host)
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
            script_name: uri.path().to_owned(),
            scheme: uri.scheme().expect("PublicUrl has a scheme").clone(),
            authority: uri.authority().expect("PublicUrl has an authority").clone(),
        }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Urls {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match parts
            .extensions
            .get::<UrlSource>()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        {
            UrlSource::Explicit(url) => return Ok(Self::from(url)),
            UrlSource::Automatic => {}
        }

        let script_name = parts
            .headers
            .get("X-Script-Name")
            .map(|value| {
                value
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|_| StatusCode::BAD_GATEWAY)
            })
            .transpose()?
            .unwrap_or_else(|| "/".to_owned());
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
            .unwrap_or(Scheme::HTTP);
        let authority = parts
            .headers
            .get("X-Forwarded-Host")
            .or_else(|| parts.headers.get("Host"))
            .ok_or(StatusCode::BAD_REQUEST)?
            .to_str()
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        if authority.contains(['@', ',', '\\']) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let authority = authority
            .parse::<Authority>()
            .map_err(|_| StatusCode::BAD_REQUEST)?;

        Ok(Self {
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

    use super::{UrlSource, Urls};
    use crate::config::PublicUrl;

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
        parts.extensions.insert(UrlSource::Explicit(url));
        let urls = Urls::from_request_parts(&mut parts, &())
            .await
            .expect("configured URLs");
        assert_eq!(
            urls.external("account").expect("valid URL"),
            "https://user:p%40ss@example.com:8443/app/account"
        );
        assert_eq!(urls.internal("account"), "/app/account");
        assert_eq!(urls.public_host(), "example.com:8443");
    }

    #[tokio::test]
    async fn automatic_urls_require_registration_and_a_host() {
        let (mut parts, ()) = Request::new(()).into_parts();
        assert_eq!(
            Urls::from_request_parts(&mut parts, &())
                .await
                .expect_err("missing registration"),
            StatusCode::INTERNAL_SERVER_ERROR,
        );
        parts.extensions.insert(UrlSource::Automatic);
        assert_eq!(
            Urls::from_request_parts(&mut parts, &())
                .await
                .expect_err("missing host"),
            StatusCode::BAD_REQUEST,
        );
        parts
            .headers
            .insert("Host", "127.0.0.1:3000".parse().expect("valid header"));
        let urls = Urls::from_request_parts(&mut parts, &())
            .await
            .expect("request-derived URLs");
        assert_eq!(urls.internal("foo"), "/foo");
        assert_eq!(
            urls.external("foo").expect("valid URL"),
            "http://127.0.0.1:3000/foo"
        );
    }

    #[test]
    fn url_construction() {
        for (prefix, expected_path) in [
            ("/", "/foo?bar=baz"),
            ("/script-path/bla///", "/script-path/bla/foo?bar=baz"),
        ] {
            let urls = Urls {
                script_name: prefix.to_owned(),
                scheme: Scheme::HTTPS,
                authority: "example.com:8443".parse().expect("valid authority"),
            };
            for path in ["foo?bar=baz", "/foo?bar=baz"] {
                assert_eq!(urls.internal(path), expected_path);
                assert_eq!(
                    urls.external(path).expect("valid URL"),
                    format!("https://example.com:8443{expected_path}")
                );
            }
        }
    }
}
