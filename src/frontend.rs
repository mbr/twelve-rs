//! Supports REST APIs shipped with a separately built single-page application.
//!
//! [`cache`] lets immutable frontend assets be cached safely. [`version`] adds
//! the deployed frontend version to API responses so the running application
//! can detect a newer deployment. The two middleware can be used independently.
//!
//! A typical setup reuses the frontend root for file serving and locating the
//! version manifest.
//!
//! ```no_run
//! use std::path::PathBuf;
//!
//! use axum::{Router, routing::get};
//! use tower_http::services::ServeDir;
//! use twelve::frontend::RouterExt;
//!
//! let root = PathBuf::from("/srv/frontend");
//! let api = Router::new()
//!     .route("/status", get(|| async { "ok" }))
//!     .with_frontend_version(&root);
//! let frontend = Router::new()
//!     .fallback_service(ServeDir::new(&root).append_index_html_on_directories(true))
//!     .with_frontend_cache();
//! let application: Router = Router::new().nest("/api", api).merge(frontend);
//! ```

use std::path::Path;

use axum::{middleware, Router};

pub mod cache;
pub mod version;

/// Adds independently usable frontend middleware to Axum routers.
pub trait RouterExt: Sized {
    /// Applies frontend cache policy to this router.
    fn with_frontend_cache(self) -> Self;

    /// Advertises the version stored at the frontend root on this router's
    /// responses.
    fn with_frontend_version(self, frontend: impl AsRef<Path>) -> Self;
}

impl<S> RouterExt for Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    #[inline]
    fn with_frontend_cache(self) -> Self {
        self.layer(middleware::from_fn(cache::set))
    }

    #[inline]
    fn with_frontend_version(self, frontend: impl AsRef<Path>) -> Self {
        let version = version::FrontendVersion::new(frontend.as_ref().to_owned());
        self.layer(middleware::from_fn_with_state(version, version::attach))
    }
}
