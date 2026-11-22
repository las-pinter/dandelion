//! Server configuration.
//!
//! Dandelion binds to loopback only until authentication and TLS exist. The
//! address is validated before the socket is opened, so a misconfiguration
//! fails loudly instead of exposing the API on the network.

use std::{
    env,
    ffi::OsString,
    fmt,
    net::{AddrParseError, Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
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

/// Why a data directory could not be chosen.
#[derive(Debug, Error)]
pub enum DataDirError {
    /// No explicit value and no usable environment variable.
    #[error("cannot choose a data directory: set DANDELION_DATA_DIR, XDG_DATA_HOME, or HOME")]
    NoCandidate,
    /// An explicit path was supplied but is empty.
    #[error("the data directory path is empty")]
    Empty,
}

/// Resolves the data directory.
///
/// Order: `DANDELION_DATA_DIR`, `$XDG_DATA_HOME/dandelion`, then
/// `$HOME/.local/share/dandelion`. An explicit `--data-dir` wins over all.
pub fn data_dir(explicit: Option<PathBuf>) -> Result<PathBuf, DataDirError> {
    choose_data_dir(
        explicit,
        env::var_os("DANDELION_DATA_DIR"),
        env::var_os("XDG_DATA_HOME"),
        env::var_os("HOME"),
    )
}

fn choose_data_dir(
    explicit: Option<PathBuf>,
    env_dir: Option<OsString>,
    xdg: Option<OsString>,
    home: Option<OsString>,
) -> Result<PathBuf, DataDirError> {
    if let Some(dir) = explicit {
        if dir.as_os_str().is_empty() {
            return Err(DataDirError::Empty);
        }
        return Ok(dir);
    }
    if let Some(dir) = non_empty(env_dir) {
        return Ok(PathBuf::from(dir));
    }
    if let Some(dir) = non_empty(xdg) {
        return Ok(PathBuf::from(dir).join("dandelion"));
    }
    if let Some(dir) = non_empty(home) {
        return Ok(PathBuf::from(dir).join(".local/share/dandelion"));
    }

    Err(DataDirError::NoCandidate)
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|value| !value.is_empty())
}

/// The SQLite file inside the data directory.
#[must_use]
pub fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join("dandelion.db")
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
    };

    use super::{BindAddr, BindError, DataDirError, choose_data_dir, database_path};

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

    // Mirrors the `Option<OsString>` parameters so the cases stay readable.
    #[allow(clippy::unnecessary_wraps)]
    fn os(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    #[test]
    fn explicit_data_dir_wins() {
        let chosen = choose_data_dir(
            Some(PathBuf::from("/explicit")),
            os("/env"),
            os("/xdg"),
            os("/home"),
        );

        assert_eq!(chosen.ok(), Some(PathBuf::from("/explicit")));
    }

    #[test]
    fn empty_explicit_path_is_rejected() {
        assert!(matches!(
            choose_data_dir(Some(PathBuf::new()), None, None, None),
            Err(DataDirError::Empty)
        ));
    }

    #[test]
    fn env_data_dir_wins_over_xdg_and_home() {
        let chosen = choose_data_dir(None, os("/env"), os("/xdg"), os("/home"));

        assert_eq!(chosen.ok(), Some(PathBuf::from("/env")));
    }

    #[test]
    fn xdg_data_dir_appends_dandelion() {
        let chosen = choose_data_dir(None, None, os("/xdg"), os("/home"));

        assert_eq!(chosen.ok(), Some(PathBuf::from("/xdg/dandelion")));
    }

    #[test]
    fn home_fallback_appends_local_share() {
        let chosen = choose_data_dir(None, None, None, os("/home"));

        assert_eq!(
            chosen.ok(),
            Some(PathBuf::from("/home/.local/share/dandelion"))
        );
    }

    #[test]
    fn empty_values_are_ignored() {
        let chosen = choose_data_dir(None, os(""), os(""), os("/home"));

        assert_eq!(
            chosen.ok(),
            Some(PathBuf::from("/home/.local/share/dandelion"))
        );
    }

    #[test]
    fn no_candidate_is_an_error() {
        assert!(matches!(
            choose_data_dir(None, None, None, None),
            Err(DataDirError::NoCandidate)
        ));
    }

    #[test]
    fn database_path_appends_the_file_name() {
        assert_eq!(
            database_path(Path::new("/var/lib/dandelion")),
            PathBuf::from("/var/lib/dandelion/dandelion.db")
        );
    }
}
