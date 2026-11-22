//! Server configuration.
//!
//! Dandelion binds to loopback only until authentication and TLS exist. The
//! address is validated before the socket is opened, so a misconfiguration
//! fails loudly instead of exposing the API on the network.

use std::{
    fmt,
    net::{AddrParseError, Ipv4Addr, SocketAddr, SocketAddrV4},
    str::FromStr,
};

use thiserror::Error;

/// A validated loopback listen address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindAddr(SocketAddr);

/// Why a listen address was rejected.
#[derive(Debug, Error)]
pub enum BindError {
    /// The text is not a socket address.
    #[error("invalid bind address `{value}`")]
    Invalid {
        /// The text that failed to parse.
        value: String,
        /// The parser's reason.
        #[source]
        source: AddrParseError,
    },
    /// The address would expose the server beyond the machine.
    #[error(
        "refusing non-loopback bind address `{0}`; dandelion binds to loopback until auth and TLS exist"
    )]
    NonLoopback(SocketAddr),
}

impl BindAddr {
    /// Loopback on the default HTTP port.
    pub const DEFAULT: Self = Self(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 7443)));

    /// Returns the socket address to bind.
    #[must_use]
    pub const fn socket_addr(self) -> SocketAddr {
        self.0
    }
}

impl FromStr for BindAddr {
    type Err = BindError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let address: SocketAddr = raw.parse().map_err(|source| BindError::Invalid {
            value: raw.to_owned(),
            source,
        })?;
        if !address.ip().is_loopback() {
            return Err(BindError::NonLoopback(address));
        }
        Ok(Self(address))
    }
}

impl fmt::Display for BindAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::{BindAddr, BindError};

    fn parse(raw: &str) -> Result<BindAddr, BindError> {
        raw.parse()
    }

    #[test]
    fn accepts_ipv4_loopback() {
        assert_eq!(parse("127.0.0.1:7443").ok(), Some(BindAddr::DEFAULT));
    }

    #[test]
    fn accepts_ipv6_loopback() {
        assert!(parse("[::1]:7443").is_ok());
    }

    #[test]
    fn rejects_wildcard_address() {
        assert!(matches!(
            parse("0.0.0.0:7443"),
            Err(BindError::NonLoopback(_))
        ));
    }

    #[test]
    fn rejects_network_address() {
        assert!(matches!(
            parse("192.168.1.10:7443"),
            Err(BindError::NonLoopback(_))
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            parse("not-an-address"),
            Err(BindError::Invalid { .. })
        ));
    }

    #[test]
    fn displays_the_socket_address() {
        assert_eq!(BindAddr::DEFAULT.to_string(), "127.0.0.1:7443");
    }
}
