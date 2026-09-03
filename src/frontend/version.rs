//! Adds a frontend version file to HTTP responses.
//!
//! The middleware reads `frontend-version` from the frontend root for each
//! request and sets its contents as the `frontend-version` response header.
//! The file must contain a valid header value. Failures are logged and do not
//! set the header.
//!
//! Apply the middleware to the router that should report the version:
//!
//! ```no_run
//! use std::path::PathBuf;
//!
//! use axum::Router;
//! use twelve::frontend::RouterExt;
//!
//! let frontend = PathBuf::from("/srv/frontend");
//! let api: Router = Router::new().with_frontend_version(frontend);
//! ```

use std::{
    fs,
    path::{Path, PathBuf},
};

use axum::{
    extract::{Request, State},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

/// Names the frontend version manifest relative to the frontend root.
const MANIFEST_NAME: &str = "frontend-version";

/// Names the response header containing the frontend version.
const HEADER_NAME: &str = "frontend-version";

/// Holds the path to a generated frontend version manifest.
#[derive(Clone, Debug)]
pub struct FrontendVersion {
    /// Identifies the file read for each request.
    manifest: PathBuf,
}

impl FrontendVersion {
    /// Constructs middleware state for the frontend rooted at `frontend`.
    #[must_use]
    pub fn new(frontend: PathBuf) -> Self {
        Self {
            manifest: frontend.join(MANIFEST_NAME),
        }
    }
}

/// Adds the current frontend version when its manifest is available and valid.
pub async fn attach(
    State(frontend_version): State<FrontendVersion>,
    request: Request,
    next: Next,
) -> Response {
    let version = match read(&frontend_version.manifest) {
        Ok(version) => Some(version),
        Err(error) => {
            tracing::warn!(
                %error,
                manifest = %frontend_version.manifest.display(),
                "failed to read frontend version"
            );
            None
        }
    };
    let mut response = next.run(request).await;

    if let Some(version) = version {
        response.headers_mut().insert(HEADER_NAME, version);
    }

    response
}

/// Reads the generated frontend version manifest as a header value.
fn read(manifest: &Path) -> std::io::Result<HeaderValue> {
    fs::read_to_string(manifest)?
        .trim()
        .parse()
        .map_err(|source| std::io::Error::new(std::io::ErrorKind::InvalidData, source))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::{attach, FrontendVersion, HEADER_NAME, MANIFEST_NAME};

    /// Provides the first frontend version observed by the test application.
    const VERSION_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// Provides the second frontend version observed by the test application.
    const VERSION_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// Refreshes the header and omits it while the manifest is unavailable.
    #[tokio::test]
    async fn reads_manifest_for_each_request() {
        let frontend = tempdir().expect("temporary frontend should be created");
        let manifest = frontend.path().join(MANIFEST_NAME);
        fs::write(&manifest, VERSION_A).expect("initial manifest should be written");
        let application =
            Router::new()
                .route("/", get(|| async {}))
                .layer(middleware::from_fn_with_state(
                    FrontendVersion::new(frontend.path().to_owned()),
                    attach,
                ));

        let response = application
            .clone()
            .oneshot(request())
            .await
            .expect("initial request should complete");
        assert_eq!(response.headers()[HEADER_NAME], VERSION_A);

        fs::write(&manifest, VERSION_B).expect("updated manifest should be written");
        let response = application
            .clone()
            .oneshot(request())
            .await
            .expect("updated request should complete");
        assert_eq!(response.headers()[HEADER_NAME], VERSION_B);

        fs::remove_file(manifest).expect("manifest should be removed");
        let response = application
            .oneshot(request())
            .await
            .expect("request without manifest should complete");
        assert!(response.headers().get(HEADER_NAME).is_none());
    }

    /// Builds a request for the middleware test application.
    fn request() -> Request<Body> {
        Request::builder()
            .uri("/")
            .body(Body::empty())
            .expect("request should be built")
    }
}
