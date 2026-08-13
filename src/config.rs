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
    ffi::{OsStr, OsString},
    fmt::{self, Display, Formatter},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use thiserror::Error;

/// Identifies the source of a configuration document.
#[derive(Debug, Eq, PartialEq)]
pub enum Location {
    /// Reads configuration from standard input.
    StandardInput,

    /// Reads configuration from a file.
    File(PathBuf),
}

impl Location {
    /// Reads the configuration document into a string.
    fn read_to_string(&self) -> io::Result<String> {
        match self {
            Self::StandardInput => {
                let mut serialized = String::new();
                io::stdin()
                    .lock()
                    .read_to_string(&mut serialized)
                    .map(|_| serialized)
            }
            Self::File(path) => fs::read_to_string(path),
        }
    }
}

impl From<OsString> for Location {
    /// Converts a process argument into a configuration location.
    fn from(value: OsString) -> Self {
        if value.as_os_str() == OsStr::new("-") {
            Self::StandardInput
        } else {
            Self::File(value.into())
        }
    }
}

impl Display for Location {
    /// Formats the location for diagnostics.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::StandardInput => formatter.write_str("standard input"),
            Self::File(path) => path.display().fmt(formatter),
        }
    }
}

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
        location: Location,

        /// Provides the underlying input error.
        #[source]
        source: io::Error,
    },

    /// Indicates that the configuration could not be deserialized.
    #[error("failed to parse configuration from {location}")]
    Parse {
        /// Identifies the configuration source.
        location: Location,

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
    let mut arguments = env::args_os().skip(1);
    let path = arguments.next().ok_or(Error::MissingPath)?;
    if arguments.next().is_some() {
        return Err(Error::UnexpectedArgument);
    }

    load_location(path.into())
}

/// Loads application configuration from a TOML file or standard input.
///
/// A path equal to `-` reads the document from standard input.
pub fn load<T>(path: &Path) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    load_location(path.as_os_str().to_owned().into())
}

/// Loads application configuration from a resolved location.
fn load_location<T>(location: Location) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let serialized = match location.read_to_string() {
        Ok(serialized) => serialized,
        Err(source) => return Err(Error::Read { location, source }),
    };

    deserialize(&serialized, location)
}

/// Deserializes a TOML document with source-aware diagnostics.
fn deserialize<T>(serialized: &str, location: Location) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    toml::from_str(serialized).map_err(|source| Error::Parse { location, source })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde::Deserialize;

    use super::{deserialize, Location};

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
        let config: Config = deserialize(
            "[server]\nport = 3000\n",
            Location::File(PathBuf::from("test")),
        )
        .expect("configuration should deserialize");

        assert_eq!(
            config,
            Config {
                server: Server { port: 3000 },
            }
        );
    }
}
