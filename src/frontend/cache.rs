//! Applies cache policy to a separately built frontend.
//!
//! The frontend must reserve `/static/` for public assets whose contents never
//! change at a published path. Mutable files must remain outside `/static/`,
//! and the middleware must only wrap the router serving frontend files.
//!
//! Successful static asset responses are cached as immutable. Other frontend
//! responses require revalidation.
//!
//! Apply the middleware to the frontend router:
//!
//! ```
//! use axum::Router;
//! use twelve::frontend::RouterExt;
//!
//! let frontend: Router = Router::new().with_frontend_cache();
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
