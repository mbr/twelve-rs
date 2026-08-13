//! Loads structured deployment configuration from TOML.
//!
//! The twelve-factor methodology recommends environment variables for
//! configuration. This module uses deployment-supplied TOML instead. It keeps
//! configuration separate from source and builds while making nested values
//! and collections straightforward to express.
//!
//! Applications with flat environment-based configuration may prefer
//! [`envy`](https://docs.rs/envy).

use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use thiserror::Error;

/// Describes a failure to resolve or load application configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that no configuration source was provided.
    #[error("configuration file path is required")]
    MissingPath,

    /// Indicates that more than one configuration source was provided.
    #[error("unexpected argument after configuration file path")]
    UnexpectedArgument,

    /// Indicates that the configuration source could not be read.
    #[error("failed to read configuration from {location}")]
    Read {
        /// Identifies the configuration source.
        location: String,

        /// Provides the underlying input error.
        #[source]
        source: io::Error,
    },

    /// Indicates that the configuration could not be deserialized.
    #[error("failed to parse configuration from {location}")]
    Parse {
        /// Identifies the configuration source.
        location: String,

        /// Provides the underlying TOML error.
        #[source]
        source: toml::de::Error,
    },
}

/// Loads application configuration from the sole process argument.
///
/// The argument identifies a TOML file, or standard input when it is `-`.
pub fn from_args<T>() -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let path = configuration_path(env::args_os().skip(1))?;
    load(&path)
}

/// Loads application configuration from a TOML file or standard input.
///
/// A path equal to `-` reads the document from standard input.
pub fn load<T>(path: &Path) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let location = location(path);
    let serialized = if path == Path::new("-") {
        let mut serialized = String::new();
        io::stdin()
            .lock()
            .read_to_string(&mut serialized)
            .map_err(|source| Error::Read {
                location: location.clone(),
                source,
            })?;
        serialized
    } else {
        fs::read_to_string(path).map_err(|source| Error::Read {
            location: location.clone(),
            source,
        })?
    };

    deserialize(&serialized, location)
}

/// Selects the sole configuration path from process arguments.
fn configuration_path(mut arguments: impl Iterator<Item = OsString>) -> Result<PathBuf, Error> {
    let path = arguments.next().ok_or(Error::MissingPath)?;
    if arguments.next().is_some() {
        return Err(Error::UnexpectedArgument);
    }

    Ok(path.into())
}

/// Describes a configuration source for diagnostics.
fn location(path: &Path) -> String {
    if path == Path::new("-") {
        "standard input".to_owned()
    } else {
        path.display().to_string()
    }
}

/// Deserializes a TOML document with source-aware diagnostics.
fn deserialize<T>(serialized: &str, location: String) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    toml::from_str(serialized).map_err(|source| Error::Parse { location, source })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use serde::Deserialize;

    use super::{configuration_path, deserialize, Error};

    /// Provides a nested configuration fixture.
    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Config {
        /// Configures the server fixture.
        server: Server,
    }

    /// Provides nested server fields for the configuration fixture.
    #[derive(Debug, Deserialize, Eq, PartialEq)]
    struct Server {
        /// Selects the test listener port.
        port: u16,
    }

    /// Deserializes structured TOML configuration.
    #[test]
    fn deserializes_structured_configuration() {
        let config: Config = deserialize("[server]\nport = 3000\n", "test".to_owned())
            .expect("configuration should deserialize");

        assert_eq!(
            config,
            Config {
                server: Server { port: 3000 },
            }
        );
    }

    /// Requires exactly one configuration path.
    #[test]
    fn validates_configuration_arguments() {
        let missing = configuration_path(Vec::new().into_iter());
        assert!(matches!(missing, Err(Error::MissingPath)));

        let extra = configuration_path(
            [OsString::from("config.toml"), OsString::from("extra")].into_iter(),
        );
        assert!(matches!(extra, Err(Error::UnexpectedArgument)));
    }
}
