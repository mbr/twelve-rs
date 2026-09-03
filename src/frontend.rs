//! Supports separately built browser frontends.
//!
//! [`cache`] applies cache policy to content-addressed static assets, while
//! [`version`] advertises the deployed frontend version on selected responses.
//! Applications can use either module independently.

pub mod cache;
pub mod version;
