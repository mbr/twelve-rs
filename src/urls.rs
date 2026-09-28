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
//! Handlers can instead take `urls: Urls` as an Axum extractor. Pass
//! `config.url_source()` to an [`axum::Extension`] layer to use the configured
//! URL, falling back to automatic detection when `public_url` is absent:
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
//! # Internal URLs only
//!
//! Use `Urls::internal_only("/app")` without a public origin, or register
//! `Extension(UrlSource::internal("/app"))` for handlers. `internal()` and
//! redirects work normally; `external()` returns an error. Invalid prefixes
//! panic at construction.
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
        uri::{Authority, PathAndQuery, Scheme},
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
    /// Uses only a path prefix, without consulting request headers.
    InternalOnly(String),
}

impl UrlSource {
    /// Selects internal-only URLs with an absolute path prefix.
    ///
    /// # Panics
    ///
    /// Panics if the prefix is not a URI-encoded absolute path, or contains an
    /// authority, query, or fragment.
    pub fn internal<S: Into<String>>(prefix: S) -> Self {
        Self::InternalOnly(Urls::internal_only(prefix).script_name)
    }
}

/// Reports public URL construction failures.
#[derive(Debug, Error)]
pub enum ExternalUrlError {
    /// Indicates that this URL generator only supports internal URLs.
    #[error("external URLs are unavailable in internal-only mode")]
    InternalOnly,
    /// Indicates that the supplied path or prefix is not a valid URI.
    #[error("invalid public URL")]
    InvalidUri(#[source] axum::http::Error),
}

/// Identifies whether an origin is available for external URLs.
#[derive(Clone, Debug)]
enum Origin {
    /// Supports only paths relative to the application prefix.
    InternalOnly,
    /// Provides a resolved public origin.
    External {
        /// Identifies the HTTP scheme.
        scheme: Scheme,
        /// Includes the public host, credentials, and port.
        authority: Authority,
    },
}

/// Constructs URLs from a public address or an internal-only prefix.
#[derive(Clone, Debug)]
pub struct Urls {
    /// The absolute path on the domain that the app is running under.
    script_name: String,
    /// Determines whether absolute URL construction is available.
    origin: Origin,
}

impl Urls {
    /// Constructs an internal-only URL generator with an absolute path prefix.
    ///
    /// # Panics
    ///
    /// Panics if the prefix is not a URI-encoded absolute path, or contains an
    /// authority, query, or fragment.
    pub fn internal_only<S: Into<String>>(prefix: S) -> Self {
        let prefix = prefix.into();
        assert!(
            prefix.starts_with('/')
                && !prefix.starts_with("//")
                && !prefix.contains(['?', '#', '\\'])
                && !prefix.bytes().any(|byte| byte.is_ascii_whitespace())
                && prefix.parse::<PathAndQuery>().is_ok(),
            "invalid internal URL prefix"
        );
        Self {
            script_name: prefix,
            origin: Origin::InternalOnly,
        }
    }

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
    /// Accepts URI-encoded paths with or without a leading slash. Fails in
    /// internal-only mode or if the resulting URI is invalid.
    pub fn external<S: AsRef<str>>(&self, path: S) -> Result<String, ExternalUrlError> {
        let Origin::External { scheme, authority } = &self.origin else {
            return Err(ExternalUrlError::InternalOnly);
        };
        Uri::builder()
            .scheme(scheme.clone())
            .authority(authority.clone())
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

    /// Returns the public host and optional port, or `None` in internal-only mode.
    pub fn public_host(&self) -> Option<&str> {
        let Origin::External { authority, .. } = &self.origin else {
            return None;
        };
        let authority = authority.as_str();
        Some(
            authority
                .rsplit_once('@')
                .map_or(authority, |(_, host)| host),
        )
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
            origin: Origin::External {
                scheme: uri.scheme().expect("PublicUrl has a scheme").clone(),
                authority: uri.authority().expect("PublicUrl has an authority").clone(),
            },
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
            UrlSource::InternalOnly(prefix) => return Ok(Self::internal_only(prefix.clone())),
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
            origin: Origin::External { scheme, authority },
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::FromRequestParts,
        http::{uri::Scheme, Request, StatusCode},
    };

    use super::{ExternalUrlError, Origin, UrlSource, Urls};
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
        assert_eq!(urls.public_host(), Some("example.com:8443"));
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

    #[tokio::test]
    async fn internal_only_urls_need_no_origin() {
        let urls = Urls::internal_only("/app");
        assert_eq!(urls.internal("account"), "/app/account");
        assert!(urls.public_host().is_none());
        assert!(matches!(
            urls.external("account"),
            Err(ExternalUrlError::InternalOnly)
        ));

        let (mut parts, ()) = Request::builder()
            .header("X-Script-Name", "/ignored")
            .body(())
            .expect("valid request")
            .into_parts();
        parts.extensions.insert(UrlSource::internal("/"));
        let urls = Urls::from_request_parts(&mut parts, &())
            .await
            .expect("internal URLs");
        assert_eq!(urls.internal("account"), "/account");
    }

    #[test]
    #[should_panic(expected = "invalid internal URL prefix")]
    fn internal_only_rejects_non_path_prefixes() {
        Urls::internal_only("https://example.com/app");
    }

    #[test]
    fn url_construction() {
        for (prefix, expected_path) in [
            ("/", "/foo?bar=baz"),
            ("/script-path/bla///", "/script-path/bla/foo?bar=baz"),
        ] {
            let urls = Urls {
                script_name: prefix.to_owned(),
                origin: Origin::External {
                    scheme: Scheme::HTTPS,
                    authority: "example.com:8443".parse().expect("valid authority"),
                },
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
