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
    fmt::{self, Display, Formatter},
    fs,
    io::{self, Read},
    net::{AddrParseError, SocketAddr},
    num::ParseIntError,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{de::DeserializeOwned, Deserialize};
use thiserror::Error;
use tracing_subscriber::{filter::ParseError, EnvFilter};

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

impl From<PathBuf> for Location {
    /// Converts a path into a configuration location.
    fn from(path: PathBuf) -> Self {
        if path.as_path() == Path::new("-") {
            Self::StandardInput
        } else {
            Self::File(path)
        }
    }
}

impl From<OsString> for Location {
    /// Converts a process argument into a configuration location.
    fn from(value: OsString) -> Self {
        PathBuf::from(value).into()
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

/// Identifies an HTTP listener.
///
/// Numeric IPv4 and IPv6 socket addresses select TCP. Absolute filesystem
/// paths select Unix-domain sockets. A `fd://N` address selects a listening
/// file descriptor inherited through systemd socket activation. Hostnames and
/// relative paths are not accepted.
///
/// ```
/// use twelve::config::ListenAddress;
///
/// let ipv4: ListenAddress = "127.0.0.1:3000".parse()?;
/// let ipv6: ListenAddress = "[::1]:3000".parse()?;
/// let unix: ListenAddress = "/run/myapp/http.sock".parse()?;
/// let inherited: ListenAddress = "fd://3".parse()?;
///
/// assert!(matches!(ipv4, ListenAddress::Tcp(_)));
/// assert!(matches!(ipv6, ListenAddress::Tcp(_)));
/// assert!(matches!(unix, ListenAddress::Unix(_)));
/// assert_eq!(inherited, ListenAddress::FileDescriptor(3));
/// # Ok::<(), twelve::config::ParseListenAddressError>(())
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "String")]
pub enum ListenAddress {
    /// Listens on a TCP socket.
    Tcp(SocketAddr),

    /// Listens on a Unix-domain socket.
    Unix(PathBuf),

    /// Uses an inherited listening file descriptor.
    FileDescriptor(i32),
}

impl FromStr for ListenAddress {
    type Err = ParseListenAddressError;

    /// Parses a TCP address, Unix socket path, or inherited descriptor.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Some(descriptor) = value.strip_prefix("fd://") {
            let descriptor = descriptor
                .parse()
                .map_err(|source| ParseListenAddressError::FileDescriptor { source })?;

            if descriptor < 3 {
                return Err(ParseListenAddressError::ReservedFileDescriptor { descriptor });
            }

            Ok(Self::FileDescriptor(descriptor))
        } else if Path::new(value).is_absolute() {
            Ok(Self::Unix(PathBuf::from(value)))
        } else {
            value
                .parse()
                .map(Self::Tcp)
                .map_err(|source| ParseListenAddressError::Address { source })
        }
    }
}

impl TryFrom<String> for ListenAddress {
    type Error = ParseListenAddressError;

    /// Parses an owned listener address.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Display for ListenAddress {
    /// Formats the listener address.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(address) => address.fmt(formatter),
            Self::Unix(path) => path.display().fmt(formatter),
            Self::FileDescriptor(descriptor) => write!(formatter, "fd://{descriptor}"),
        }
    }
}

/// Describes an invalid HTTP listener address.
#[derive(Debug, Error)]
pub enum ParseListenAddressError {
    /// Indicates that a direct listener address is invalid.
    #[error("expected a TCP socket address, absolute Unix socket path, or fd://N")]
    Address {
        /// Provides the underlying TCP address error.
        #[source]
        source: AddrParseError,
    },

    /// Indicates that an inherited descriptor is not an integer.
    #[error("invalid inherited listener descriptor: {source}")]
    FileDescriptor {
        /// Provides the integer parsing error.
        #[source]
        source: ParseIntError,
    },

    /// Indicates that a descriptor is reserved for standard process streams.
    #[error("inherited listener descriptor must be 3 or greater, received {descriptor}")]
    ReservedFileDescriptor {
        /// Provides the invalid descriptor.
        descriptor: i32,
    },
}

/// Holds a validated tracing filter.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct LogFilter(EnvFilter);

impl FromStr for LogFilter {
    type Err = ParseError;

    /// Parses a tracing filter.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        EnvFilter::try_new(value).map(Self)
    }
}

impl TryFrom<String> for LogFilter {
    type Error = ParseError;

    /// Parses an owned tracing filter.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<LogFilter> for EnvFilter {
    /// Extracts the validated tracing filter.
    fn from(filter: LogFilter) -> Self {
        filter.0
    }
}

impl Display for LogFilter {
    /// Formats the tracing filter.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Provides configuration shared by web applications.
///
/// This can be flattened into application-specific Serde configuration.
#[derive(Debug, Deserialize)]
pub struct Core {
    /// Selects the address on which the HTTP server listens.
    pub listen_address: ListenAddress,

    /// Selects the tracing events emitted by the application.
    pub log_filter: LogFilter,
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
    load_location(path.to_owned().into())
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
    use std::{net::SocketAddr, path::PathBuf};

    use serde::Deserialize;

    use super::{deserialize, Core, ListenAddress, Location};

    /// Provides application-specific fields around shared configuration.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Config {
        /// Provides shared web application configuration.
        #[serde(flatten)]
        core: Core,

        /// Selects an application-specific frontend directory.
        frontend: PathBuf,
    }

    /// Deserializes flattened shared configuration.
    #[test]
    fn deserializes_flattened_core_configuration() {
        let config: Config = deserialize(
            concat!(
                "listen_address = '127.0.0.1:3000'\n",
                "log_filter = 'twelve=debug,tower_http=info'\n",
                "frontend = '/srv/frontend'\n",
            ),
            Location::File(PathBuf::from("test")),
        )
        .expect("configuration should deserialize");

        assert_eq!(
            config.core.listen_address,
            ListenAddress::Tcp(SocketAddr::from(([127, 0, 0, 1], 3000)))
        );
        assert_eq!(
            config.core.log_filter.to_string(),
            "tower_http=info,twelve=debug"
        );
        assert_eq!(config.frontend, PathBuf::from("/srv/frontend"));
    }

    /// Parses each supported listener address family.
    #[test]
    fn parses_listener_addresses() {
        let ipv6: ListenAddress = "[::1]:3000".parse().expect("IPv6 listener should parse");
        let unix: ListenAddress = "/run/myapp/http.sock"
            .parse()
            .expect("Unix listener should parse");
        let inherited: ListenAddress = "fd://7".parse().expect("inherited descriptor should parse");

        assert_eq!(
            ipv6,
            ListenAddress::Tcp(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 3000)))
        );
        assert_eq!(
            unix,
            ListenAddress::Unix(PathBuf::from("/run/myapp/http.sock"))
        );
        assert_eq!(inherited, ListenAddress::FileDescriptor(7));
        assert_eq!(inherited.to_string(), "fd://7");
        assert!("fd://2".parse::<ListenAddress>().is_err());
        assert!("myapp.sock".parse::<ListenAddress>().is_err());
    }
}
