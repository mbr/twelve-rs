#![doc = include_str!("../README.md")]

mod app_builder;
pub mod page;
mod request_context;
pub mod util;

use serde::de::DeserializeOwned;

pub use app_builder::AppBuilder;
pub use request_context::RequestContext;

#[inline(always)]
pub fn from_env<T>() -> T
where
    T: DeserializeOwned,
{
    envy::from_env().expect("failed to parse configuration from environment")
}
