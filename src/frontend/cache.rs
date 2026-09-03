//! Applies safe cache policy to a separately built frontend.
//!
//! The frontend must reserve `/static/` for public assets whose bytes never
//! change at a published path. How those paths are versioned is up to the
//! frontend build; release directories and hashes in filenames are both valid.
//! Mutable files such as `index.html` and `frontend-version` remain outside
//! `/static/`.
//!
//! Successful `GET` and `HEAD` responses below `/static/`, as well as
//! `304 Not Modified`, receive `public, max-age=31536000, immutable`. Every
//! other response receives `no-cache`, allowing storage but requiring
//! revalidation. Classification ignores query strings, and the middleware
//! replaces any existing `Cache-Control` header.
//!
//! Apply the middleware only to the router serving frontend files so API
//! responses retain their own cache policy:
//!
//! ```
//! use axum::{Router, middleware};
//! use twelve::frontend::cache;
//!
//! let frontend: Router = Router::new().layer(middleware::from_fn(cache::set));
//! ```

use axum::{
    extract::Request,
    http::{header::CACHE_CONTROL, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};

/// Sets immutable caching for static assets and revalidation elsewhere.
pub async fn set(request: Request, next: Next) -> Response {
    let immutable = is_immutable(request.method(), request.uri().path());
    let mut response = next.run(request).await;
    let cache_control = if immutable
        && (response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED)
    {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        HeaderValue::from_static("no-cache")
    };
    response.headers_mut().insert(CACHE_CONTROL, cache_control);
    response
}

/// Reports whether a method and path identify an immutable static asset.
fn is_immutable(method: &Method, path: &str) -> bool {
    (method == Method::GET || method == Method::HEAD)
        && path
            .strip_prefix("/static/")
            .is_some_and(|asset| !asset.is_empty())
}

#[cfg(test)]
mod tests {
    use axum::http::Method;

    use super::is_immutable;

    /// Recognizes safe methods for any asset below the static namespace.
    #[test]
    fn recognizes_immutable_requests() {
        assert!(is_immutable(&Method::GET, "/static/app.js"));
        assert!(is_immutable(
            &Method::GET,
            "/static/release/images/icon.svg"
        ));
        assert!(is_immutable(&Method::HEAD, "/static/app.css"));
        assert!(!is_immutable(&Method::POST, "/static/app.js"));
        assert!(!is_immutable(&Method::GET, "/static/"));
        assert!(!is_immutable(&Method::GET, "/frontend-version"));
    }
}
