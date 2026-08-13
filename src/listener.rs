//! Opens TCP and Unix-domain listeners for Axum applications.

use std::{
    fmt::{self, Display, Formatter},
    io,
    net::SocketAddr,
    path::Path,
    pin::Pin,
    task::{Context, Poll},
};

use axum::serve::Listener as AxumListener;
#[cfg(feature = "systemd")]
use listenfd::ListenFd;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream, UnixListener, UnixStream},
};

use crate::config::ListenAddress;

/// Provides a TCP or Unix-domain listener.
///
/// Use [`Listener::bind`] to create a socket directly.
///
/// The effective address is captured after binding, so [`Listener::port`]
/// reports the assigned port when the configured TCP port was zero.
#[cfg_attr(
    feature = "systemd",
    doc = "With the `systemd` feature, [`Listener::inherit_or_bind`] first looks for a socket supplied by a service manager."
)]
pub struct Listener {
    /// Holds the transport-specific listener.
    inner: Inner,

    /// Holds the effective local address.
    local_address: Address,
}

impl Listener {
    /// Binds a listener to a configured address.
    pub async fn bind(address: &ListenAddress) -> Result<Self, Error> {
        match address {
            ListenAddress::Tcp(address) => Self::bind_tcp(*address).await,
            ListenAddress::Unix(path) => Self::bind_unix(path),
        }
    }

    /// Inherits a listener when available or binds one directly.
    #[cfg(feature = "systemd")]
    pub async fn inherit_or_bind(address: &ListenAddress) -> Result<Self, Error> {
        let mut inherited = ListenFd::from_env();

        if inherited.len() > 1 {
            return Err(Error::TooManyInherited {
                count: inherited.len(),
            });
        }

        match address {
            ListenAddress::Tcp(address) => {
                Self::inherit_or_bind_tcp(*address, &mut inherited).await
            }
            ListenAddress::Unix(path) => Self::inherit_or_bind_unix(path, &mut inherited),
        }
    }

    /// Returns the effective local address.
    #[must_use]
    pub fn local_address(&self) -> &Address {
        &self.local_address
    }

    /// Returns the effective TCP port or `None` for a Unix-domain listener.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.local_address.port()
    }

    /// Binds a TCP listener.
    async fn bind_tcp(address: SocketAddr) -> Result<Self, Error> {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|source| Error::BindTcp { address, source })?;
        let local_address = listener
            .local_addr()
            .map_err(|source| Error::ReadTcpAddress { source })?;

        Ok(Self {
            inner: Inner::Tcp(listener),
            local_address: Address::Tcp(local_address),
        })
    }

    /// Binds a Unix-domain listener.
    fn bind_unix(path: &Path) -> Result<Self, Error> {
        let listener = UnixListener::bind(path).map_err(|source| Error::BindUnix {
            path: path.to_path_buf(),
            source,
        })?;
        let local_address = listener
            .local_addr()
            .map_err(|source| Error::ReadUnixAddress { source })?;

        Ok(Self {
            inner: Inner::Unix(listener),
            local_address: Address::Unix(local_address),
        })
    }

    /// Inherits a TCP listener or binds one directly.
    #[cfg(feature = "systemd")]
    async fn inherit_or_bind_tcp(
        address: SocketAddr,
        inherited: &mut ListenFd,
    ) -> Result<Self, Error> {
        let Some(listener) = inherited
            .take_tcp_listener(0)
            .map_err(|source| Error::InheritedTcp { source })?
        else {
            return Self::bind_tcp(address).await;
        };

        let actual = listener
            .local_addr()
            .map_err(|source| Error::ReadInheritedTcpAddress { source })?;
        if !tcp_addresses_match(address, actual) {
            return Err(Error::TcpAddressMismatch {
                expected: address,
                actual,
            });
        }

        listener
            .set_nonblocking(true)
            .map_err(|source| Error::ConfigureInherited { source })?;
        let listener = TcpListener::from_std(listener)
            .map_err(|source| Error::ConfigureInherited { source })?;

        Ok(Self {
            inner: Inner::Tcp(listener),
            local_address: Address::Tcp(actual),
        })
    }

    /// Inherits a Unix-domain listener or binds one directly.
    #[cfg(feature = "systemd")]
    fn inherit_or_bind_unix(path: &Path, inherited: &mut ListenFd) -> Result<Self, Error> {
        let Some(listener) = inherited
            .take_unix_listener(0)
            .map_err(|source| Error::InheritedUnix { source })?
        else {
            return Self::bind_unix(path);
        };

        let address = listener
            .local_addr()
            .map_err(|source| Error::ReadInheritedUnixAddress { source })?;
        let actual = address.as_pathname().ok_or(Error::UnnamedUnix)?;
        if actual != path {
            return Err(Error::UnixAddressMismatch {
                expected: path.to_path_buf(),
                actual: actual.to_path_buf(),
            });
        }

        listener
            .set_nonblocking(true)
            .map_err(|source| Error::ConfigureInherited { source })?;
        let listener = UnixListener::from_std(listener)
            .map_err(|source| Error::ConfigureInherited { source })?;

        Ok(Self {
            inner: Inner::Unix(listener),
            local_address: Address::Unix(address.into()),
        })
    }
}

impl AxumListener for Listener {
    type Addr = Address;
    type Io = Connection;

    /// Accepts a connection from either transport.
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match &mut self.inner {
            Inner::Tcp(listener) => {
                let (connection, address) = AxumListener::accept(listener).await;
                (Connection::Tcp(connection), Address::Tcp(address))
            }
            Inner::Unix(listener) => {
                let (connection, address) = AxumListener::accept(listener).await;
                (Connection::Unix(connection), Address::Unix(address))
            }
        }
    }

    /// Returns the captured local address.
    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local_address.clone())
    }
}

/// Holds a transport-specific listener.
enum Inner {
    /// Holds a TCP listener.
    Tcp(TcpListener),

    /// Holds a Unix-domain listener.
    Unix(UnixListener),
}

/// Identifies a TCP or Unix-domain socket endpoint.
#[derive(Clone, Debug)]
pub enum Address {
    /// Identifies a TCP endpoint.
    Tcp(SocketAddr),

    /// Identifies a Unix-domain endpoint.
    Unix(tokio::net::unix::SocketAddr),
}

impl Address {
    /// Returns the TCP port or `None` for a Unix-domain endpoint.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match self {
            Self::Tcp(address) => Some(address.port()),
            Self::Unix(_) => None,
        }
    }

    /// Returns the Unix socket path when the endpoint has one.
    #[must_use]
    pub fn as_pathname(&self) -> Option<&Path> {
        match self {
            Self::Tcp(_) => None,
            Self::Unix(address) => address.as_pathname(),
        }
    }
}

impl Display for Address {
    /// Formats a TCP address or Unix socket path.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(address) => address.fmt(formatter),
            Self::Unix(address) => match address.as_pathname() {
                Some(path) => path.display().fmt(formatter),
                None => write!(formatter, "{address:?}"),
            },
        }
    }
}

/// Provides an accepted TCP or Unix-domain connection.
#[derive(Debug)]
pub enum Connection {
    /// Provides a TCP connection.
    Tcp(TcpStream),

    /// Provides a Unix-domain connection.
    Unix(UnixStream),
}

impl AsyncRead for Connection {
    /// Attempts to read bytes from the connection.
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tcp(connection) => Pin::new(connection).poll_read(context, buffer),
            Self::Unix(connection) => Pin::new(connection).poll_read(context, buffer),
        }
    }
}

impl AsyncWrite for Connection {
    /// Attempts to write bytes to the connection.
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            Self::Tcp(connection) => Pin::new(connection).poll_write(context, buffer),
            Self::Unix(connection) => Pin::new(connection).poll_write(context, buffer),
        }
    }

    /// Attempts to flush the connection.
    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            Self::Tcp(connection) => Pin::new(connection).poll_flush(context),
            Self::Unix(connection) => Pin::new(connection).poll_flush(context),
        }
    }

    /// Attempts to shut down the connection.
    fn poll_shutdown(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            Self::Tcp(connection) => Pin::new(connection).poll_shutdown(context),
            Self::Unix(connection) => Pin::new(connection).poll_shutdown(context),
        }
    }

    /// Reports whether vectored writes are supported.
    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Tcp(connection) => connection.is_write_vectored(),
            Self::Unix(connection) => connection.is_write_vectored(),
        }
    }

    /// Attempts to write multiple buffers to the connection.
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffers: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            Self::Tcp(connection) => Pin::new(connection).poll_write_vectored(context, buffers),
            Self::Unix(connection) => Pin::new(connection).poll_write_vectored(context, buffers),
        }
    }
}

/// Describes a failure to acquire a listener.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that more than one inherited descriptor was supplied.
    #[cfg(feature = "systemd")]
    #[error("expected at most one inherited listening descriptor, received {count}")]
    TooManyInherited {
        /// Provides the number of inherited descriptors.
        count: usize,
    },

    /// Indicates that an inherited descriptor was not a TCP listener.
    #[cfg(feature = "systemd")]
    #[error("failed to inherit TCP listener")]
    InheritedTcp {
        /// Provides the descriptor validation error.
        #[source]
        source: io::Error,
    },

    /// Indicates that an inherited descriptor was not a Unix listener.
    #[cfg(feature = "systemd")]
    #[error("failed to inherit Unix listener")]
    InheritedUnix {
        /// Provides the descriptor validation error.
        #[source]
        source: io::Error,
    },

    /// Indicates that an inherited TCP listener address could not be read.
    #[cfg(feature = "systemd")]
    #[error("failed to read inherited TCP listener address")]
    ReadInheritedTcpAddress {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that an inherited Unix listener address could not be read.
    #[cfg(feature = "systemd")]
    #[error("failed to read inherited Unix listener address")]
    ReadInheritedUnixAddress {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that an inherited TCP listener does not match configuration.
    #[cfg(feature = "systemd")]
    #[error(
        "inherited TCP listener address {actual} does not match configured address {expected}"
    )]
    TcpAddressMismatch {
        /// Provides the configured address.
        expected: SocketAddr,

        /// Provides the inherited address.
        actual: SocketAddr,
    },

    /// Indicates that an inherited Unix listener has no filesystem path.
    #[cfg(feature = "systemd")]
    #[error("inherited Unix listener does not have a filesystem path")]
    UnnamedUnix,

    /// Indicates that an inherited Unix listener does not match configuration.
    #[cfg(feature = "systemd")]
    #[error("inherited Unix listener path {actual} does not match configured path {expected}")]
    UnixAddressMismatch {
        /// Provides the configured path.
        expected: std::path::PathBuf,

        /// Provides the inherited path.
        actual: std::path::PathBuf,
    },

    /// Indicates that an inherited listener could not be configured for Tokio.
    #[cfg(feature = "systemd")]
    #[error("failed to configure inherited listener as nonblocking")]
    ConfigureInherited {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that a TCP listener could not be bound.
    #[error("failed to bind TCP listener at {address}")]
    BindTcp {
        /// Provides the configured address.
        address: SocketAddr,

        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that a Unix listener could not be bound.
    #[error("failed to bind Unix listener at {path}")]
    BindUnix {
        /// Provides the configured path.
        path: std::path::PathBuf,

        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that a TCP listener address could not be read.
    #[error("failed to read TCP listener address")]
    ReadTcpAddress {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that a Unix listener address could not be read.
    #[error("failed to read Unix listener address")]
    ReadUnixAddress {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },
}

/// Compares a configured TCP address with an effective address.
#[cfg(feature = "systemd")]
fn tcp_addresses_match(mut expected: SocketAddr, actual: SocketAddr) -> bool {
    if expected.port() == 0 {
        expected.set_port(actual.port());
    }

    expected == actual
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::{IpAddr, Ipv4Addr, SocketAddr},
        path::PathBuf,
        process,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::Listener;
    use crate::config::ListenAddress;

    /// Verifies that ephemeral TCP ports are reported after binding.
    #[tokio::test]
    async fn reports_effective_tcp_port() {
        let address = ListenAddress::Tcp(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0));
        let listener = Listener::bind(&address)
            .await
            .expect("ephemeral TCP listener should bind");

        assert_ne!(listener.port(), Some(0));
        assert!(listener.port().is_some());
    }

    /// Verifies the effective address of a Unix-domain listener.
    #[tokio::test]
    async fn reports_unix_socket_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time should follow the Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("twelve-listener-{}-{unique}.sock", process::id()));
        let socket = SocketFile(path.clone());
        let listener = Listener::bind(&ListenAddress::Unix(path.clone()))
            .await
            .expect("Unix listener should bind");

        assert_eq!(listener.port(), None);
        assert_eq!(listener.local_address().as_pathname(), Some(path.as_path()));

        drop(listener);
        drop(socket);
    }

    /// Verifies wildcard port matching for inherited TCP listeners.
    #[cfg(feature = "systemd")]
    #[test]
    fn matches_inherited_ephemeral_ports() {
        let expected = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let actual = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 43210);
        let other = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 43210);

        assert!(super::tcp_addresses_match(expected, actual));
        assert!(!super::tcp_addresses_match(expected, other));
    }

    /// Removes a test socket from the filesystem.
    struct SocketFile(PathBuf);

    impl Drop for SocketFile {
        /// Removes the socket path when the test completes.
        fn drop(&mut self) {
            match fs::remove_file(&self.0) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("failed to remove test socket: {error}"),
            }
        }
    }
}
