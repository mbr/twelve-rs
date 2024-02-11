use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct TwelveConfig {
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,
    pub database_url: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl TwelveConfig {
    #[inline]
    pub fn from_env() -> Self {
        envy::from_env().expect("could not parse configuration from environment")
    }
}

#[inline(always)]
fn default_static_dir() -> PathBuf {
    "./_static".into()
}

#[inline(always)]
fn default_port() -> u16 {
    3000
}
