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
/// The effective address is captured after acquisition, so [`Listener::port`]
/// reports the assigned port when the configured TCP port was zero.
pub struct Listener {
    /// Holds the transport-specific listener.
    inner: Inner,

    /// Holds the effective local address.
    local_address: Address,
}

impl Listener {
    /// Acquires the configured listener.
    ///
    /// TCP and Unix addresses are bound directly. With the `systemd` feature,
    /// the systemd policy selects the sole inherited listening descriptor.
    pub async fn bind(address: &ListenAddress) -> Result<Self, Error> {
        match address {
            ListenAddress::Tcp(address) => Self::bind_tcp(*address).await,
            ListenAddress::Unix(path) => Self::bind_unix(path),
            ListenAddress::Systemd => Self::inherit(),
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

    /// Inherits the sole listener from the socket activation environment.
    #[cfg(feature = "systemd")]
    fn inherit() -> Result<Self, Error> {
        let mut inherited = ListenFd::from_env();

        match inherited.len() {
            0 => return Err(Error::NoInheritedListener),
            1 => {}
            count => return Err(Error::MultipleInheritedListeners { count }),
        }

        match inherited.take_tcp_listener(0) {
            Ok(Some(listener)) => Self::inherit_tcp(listener),
            Ok(None) => Err(Error::NoInheritedListener),
            Err(_) => match inherited.take_unix_listener(0) {
                Ok(Some(listener)) => Self::inherit_unix(listener),
                Ok(None) => Err(Error::NoInheritedListener),
                Err(source) => Err(Error::InvalidInherited { source }),
            },
        }
    }

    /// Rejects socket activation when its feature is disabled.
    #[cfg(not(feature = "systemd"))]
    fn inherit() -> Result<Self, Error> {
        Err(Error::SocketActivationDisabled)
    }

    /// Converts an inherited TCP listener for Tokio.
    #[cfg(feature = "systemd")]
    fn inherit_tcp(listener: std::net::TcpListener) -> Result<Self, Error> {
        let address = listener
            .local_addr()
            .map_err(|source| Error::ReadInheritedTcpAddress { source })?;
        listener
            .set_nonblocking(true)
            .map_err(|source| Error::ConfigureInherited { source })?;
        let listener = TcpListener::from_std(listener)
            .map_err(|source| Error::ConfigureInherited { source })?;

        Ok(Self {
            inner: Inner::Tcp(listener),
            local_address: Address::Tcp(address),
        })
    }

    /// Converts an inherited Unix-domain listener for Tokio.
    #[cfg(feature = "systemd")]
    fn inherit_unix(listener: std::os::unix::net::UnixListener) -> Result<Self, Error> {
        let address = listener
            .local_addr()
            .map_err(|source| Error::ReadInheritedUnixAddress { source })?;
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
    /// Indicates that no listener was supplied through socket activation.
    #[cfg(feature = "systemd")]
    #[error("no listener was supplied through systemd socket activation")]
    NoInheritedListener,

    /// Indicates that socket activation supplied an ambiguous listener set.
    #[cfg(feature = "systemd")]
    #[error("expected one systemd socket activation listener, received {count}")]
    MultipleInheritedListeners {
        /// Provides the number of supplied listeners.
        count: usize,
    },

    /// Indicates that an inherited descriptor is not a supported listener.
    #[cfg(feature = "systemd")]
    #[error("inherited descriptor is not a TCP or Unix listener")]
    InvalidInherited {
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

    /// Indicates that an inherited listener could not be configured for Tokio.
    #[cfg(feature = "systemd")]
    #[error("failed to configure inherited listener as nonblocking")]
    ConfigureInherited {
        /// Provides the socket error.
        #[source]
        source: io::Error,
    },

    /// Indicates that socket activation is unavailable.
    #[cfg(not(feature = "systemd"))]
    #[error("cannot use systemd socket activation without the systemd feature")]
    SocketActivationDisabled,

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
