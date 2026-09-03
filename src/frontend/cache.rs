//! Applies safe cache policy to a separately built frontend.
//!
//! Immutable assets must use paths below `/static/<version>/`, where `version`
//! is a 64-character lowercase hexadecimal digest. Successful and not-modified
//! responses at those paths are cached for one year and marked immutable. All
//! other responses require revalidation.
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
    http::{header::CACHE_CONTROL, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};

/// Sets immutable caching for versioned assets and revalidation elsewhere.
pub async fn set(request: Request, next: Next) -> Response {
    let immutable = is_versioned_asset(request.uri().path());
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

/// Reports whether a path belongs to a versioned static asset directory.
fn is_versioned_asset(path: &str) -> bool {
    let Some(path) = path.strip_prefix("/static/") else {
        return false;
    };
    let Some((version, asset)) = path.split_once('/') else {
        return false;
    };

    !asset.is_empty()
        && version.len() == 64
        && version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::is_versioned_asset;

    /// Provides a valid static asset version.
    const VERSION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// Recognizes only assets below a versioned static directory.
    #[test]
    fn recognizes_versioned_assets() {
        assert!(is_versioned_asset(&format!("/static/{VERSION}/app.js")));
        assert!(is_versioned_asset(&format!(
            "/static/{VERSION}/images/icon.svg"
        )));
        assert!(!is_versioned_asset("/frontend-version"));
        assert!(!is_versioned_asset("/static/not-a-version/app.js"));
        assert!(!is_versioned_asset(&format!("/static/{VERSION}/")));
    }
}
