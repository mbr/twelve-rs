//! Applies cache policy to a separately built frontend.
//!
//! The frontend must reserve `/static/` for public assets whose contents never
//! change at a published path. Mutable files must remain outside `/static/`,
//! and the middleware must only wrap the router serving frontend files.
//!
//! Successful static asset responses are cached as immutable. Other frontend
//! responses are not stored. This avoids file-mtime validators that cannot
//! distinguish releases produced by reproducible build systems.
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
    http::{
        header::{
            CACHE_CONTROL, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_UNMODIFIED_SINCE, LAST_MODIFIED,
        },
        HeaderValue, Method, StatusCode,
    },
    middleware::Next,
    response::Response,
};

/// Sets immutable caching for static assets and disables storage elsewhere.
pub async fn set(mut request: Request, next: Next) -> Response {
    let immutable = is_immutable(request.method(), request.uri().path());
    if !immutable {
        request.headers_mut().remove(IF_MODIFIED_SINCE);
        request.headers_mut().remove(IF_NONE_MATCH);
        request.headers_mut().remove(IF_UNMODIFIED_SINCE);
    }

    let mut response = next.run(request).await;
    let cache_control = if immutable
        && (response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED)
    {
        HeaderValue::from_static("public, max-age=31536000, immutable")
    } else {
        response.headers_mut().remove(LAST_MODIFIED);
        HeaderValue::from_static("no-store")
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
    use std::fs;

    use axum::{
        body::Body,
        http::{
            header::{CACHE_CONTROL, IF_MODIFIED_SINCE, LAST_MODIFIED},
            Method, Request, StatusCode,
        },
        middleware, Router,
    };
    use tempfile::tempdir;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    use super::{is_immutable, set};

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

    /// Prevents normalized file mtimes from validating stale entry documents.
    #[tokio::test]
    async fn prevents_stale_entry_document_revalidation() {
        let frontend = tempdir().expect("temporary frontend should be created");
        fs::write(frontend.path().join("index.html"), "<!doctype html>").expect("index");
        let application = Router::new()
            .fallback_service(ServeDir::new(frontend.path()).append_index_html_on_directories(true))
            .layer(middleware::from_fn(set));

        let response = application
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(IF_MODIFIED_SINCE, "Thu, 01 Jan 2100 00:00:00 GMT")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert!(response.headers().get(LAST_MODIFIED).is_none());
    }
}
