//! Supports separately built browser frontends.
//!
//! [`cache`] applies cache policy to immutable static assets, while
//! [`version`] advertises the deployed frontend version on selected responses.
//! [`RouterExt`] applies either middleware directly to an Axum router:
//!
//! ```no_run
//! use std::path::PathBuf;
//!
//! use axum::Router;
//! use twelve::frontend::{RouterExt, version::FrontendVersion};
//!
//! let frontend: Router = Router::new().with_frontend_cache();
//! let api: Router = Router::new().with_frontend_version(FrontendVersion::new(
//!     PathBuf::from("/srv/frontend"),
//! ));
//! ```

use axum::{middleware, Router};

pub mod cache;
pub mod version;

/// Adds independently usable frontend middleware to Axum routers.
pub trait RouterExt: Sized {
    /// Applies frontend cache policy to this router.
    fn with_frontend_cache(self) -> Self;

    /// Advertises the current frontend version on this router's responses.
    fn with_frontend_version(self, version: version::FrontendVersion) -> Self;
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
    fn with_frontend_version(self, version: version::FrontendVersion) -> Self {
        self.layer(middleware::from_fn_with_state(version, version::attach))
    }
}
