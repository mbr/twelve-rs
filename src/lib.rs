#![doc = include_str!("../README.md")]

mod app_builder;
pub mod page;
pub mod util;

use serde::de::DeserializeOwned;

pub use app_builder::AppBuilder;

#[inline(always)]
pub fn from_env<T>() -> T
where
    T: DeserializeOwned,
{
    envy::from_env().expect("failed to parse configuration from environment")
}
