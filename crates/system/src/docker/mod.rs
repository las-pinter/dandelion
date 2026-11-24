//! Docker containers through the Engine API on the local socket.
//!
//! The socket is often absent (Docker not installed, or the user not in the
//! `docker` group). That is a normal state for this app, not an error, so
//! connection problems surface as [`DockerError::Unavailable`] or an
//! availability report that a page can render.

mod stats;
mod types;

pub use types::{
    ContainerInfo, ContainerLogLine, ContainerLogsRequest, ContainerState, ContainerStats,
    ContainerStatsRequest, DockerAvailability, LogStream, RestartContainerRequest,
    StartContainerRequest, StopContainerRequest,
};

use bollard::Docker as Bollard;
use bollard::container::LogOutput;
use bollard::query_parameters::{
    ListContainersOptionsBuilder, LogsOptionsBuilder, RestartContainerOptionsBuilder,
    StatsOptionsBuilder, StopContainerOptionsBuilder,
};
use futures_util::{Stream, StreamExt};

use stats::compute_stats;

/// Errors from the Docker domain.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    /// The daemon or its socket cannot be reached.
    #[error("Docker is unavailable: {reason}")]
    Unavailable {
        /// Why the daemon could not be reached.
        reason: String,
    },
    /// The daemon answered with an error.
    #[error("Docker API error: {0}")]
    Api(String),
    /// A container name or id is not safe to put in an API path.
    #[error("invalid container reference `{name}`")]
    InvalidContainer {
        /// The rejected reference.
        name: String,
    },
    /// A stop or restart timeout does not fit Docker's integer field.
    #[error("timeout {seconds} seconds is out of range")]
    InvalidTimeout {
        /// The rejected timeout.
        seconds: u32,
    },
}

/// A Docker Engine client.
#[derive(Debug, Clone)]
pub struct Docker {
    inner: Bollard,
}

impl Docker {
    /// Connects to the default local socket, usually `/var/run/docker.sock`.
    ///
    /// # Errors
    ///
    /// Returns [`DockerError::Unavailable`] when no socket exists there.
    pub fn connect_local() -> Result<Self, DockerError> {
        Self::from_result(Bollard::connect_with_local_defaults())
    }

    /// Connects to a specific Unix socket.
    ///
    /// # Errors
    ///
    /// Returns [`DockerError::Unavailable`] when the socket path does not exist.
    pub fn connect_with_socket(path: &str) -> Result<Self, DockerError> {
        Self::from_result(Bollard::connect_with_socket(
            path,
            120,
            bollard::API_DEFAULT_VERSION,
        ))
    }

    fn from_result(result: Result<Bollard, bollard::errors::Error>) -> Result<Self, DockerError> {
        result.map(|inner| Self { inner }).map_err(map_error)
    }

    /// Pings the daemon and reports whether it answers.
    pub async fn availability(&self) -> DockerAvailability {
        match self.inner.version().await {
            Ok(version) => DockerAvailability {
                available: true,
                reason: None,
                server_version: version.version,
            },
            Err(error) => DockerAvailability {
                available: false,
                reason: Some(error.to_string()),
                server_version: None,
            },
        }
    }

    /// Lists every container, running or not.
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, DockerError> {
        let options = ListContainersOptionsBuilder::default().all(true).build();
        let containers = self
            .inner
            .list_containers(Some(options))
            .await
            .map_err(map_error)?;
        Ok(containers
            .into_iter()
            .map(ContainerInfo::from_summary)
            .collect())
    }

    /// Starts a container.
    pub async fn start(&self, request: &StartContainerRequest) -> Result<(), DockerError> {
        validate_container(&request.container)?;
        self.inner
            .start_container(&request.container, None)
            .await
            .map_err(map_error)
    }

    /// Stops a container.
    pub async fn stop(&self, request: &StopContainerRequest) -> Result<(), DockerError> {
        validate_container(&request.container)?;
        let options = stop_timeout(request.timeout_secs)?
            .map(|seconds| StopContainerOptionsBuilder::default().t(seconds).build());
        self.inner
            .stop_container(&request.container, options)
            .await
            .map_err(map_error)
    }

    /// Restarts a container.
    pub async fn restart(&self, request: &RestartContainerRequest) -> Result<(), DockerError> {
        validate_container(&request.container)?;
        let options = stop_timeout(request.timeout_secs)?
            .map(|seconds| RestartContainerOptionsBuilder::default().t(seconds).build());
        self.inner
            .restart_container(&request.container, options)
            .await
            .map_err(map_error)
    }

    /// Streams a container's logs. With `follow` set the stream stays open.
    ///
    /// # Errors
    ///
    /// Returns an error only when the container reference is invalid or the
    /// stream cannot be opened; errors during streaming arrive as items.
    pub fn logs(
        &self,
        request: &ContainerLogsRequest,
    ) -> Result<impl Stream<Item = Result<ContainerLogLine, DockerError>> + '_, DockerError> {
        validate_container(&request.container)?;
        let tail = request
            .tail
            .map_or_else(|| "all".to_owned(), |lines| lines.to_string());
        let options = LogsOptionsBuilder::default()
            .follow(request.follow)
            .stdout(true)
            .stderr(true)
            .timestamps(request.timestamps)
            .tail(&tail)
            .build();
        Ok(self
            .inner
            .logs(&request.container, Some(options))
            .map(|item| item.map(log_line).map_err(map_error)))
    }

    /// Takes one statistics sample from a container.
    pub async fn stats(
        &self,
        request: &ContainerStatsRequest,
    ) -> Result<ContainerStats, DockerError> {
        validate_container(&request.container)?;
        let options = StatsOptionsBuilder::default()
            .stream(false)
            .one_shot(true)
            .build();
        let mut samples = self.inner.stats(&request.container, Some(options));
        match samples.next().await {
            Some(Ok(raw)) => Ok(compute_stats(&request.container, &raw)),
            Some(Err(error)) => Err(map_error(error)),
            None => Err(DockerError::Api(
                "Docker returned no statistics sample".to_owned(),
            )),
        }
    }
}

fn log_line(output: LogOutput) -> ContainerLogLine {
    let (stream, message) = match output {
        LogOutput::StdErr { message } => (LogStream::Stderr, message),
        LogOutput::StdIn { message } => (LogStream::Stdin, message),
        LogOutput::Console { message } => (LogStream::Console, message),
        LogOutput::StdOut { message } => (LogStream::Stdout, message),
    };
    ContainerLogLine {
        stream,
        message: String::from_utf8_lossy(&message).into_owned(),
    }
}

/// Checks that a container name or id is safe to put in an API path.
///
/// Docker builds request paths from these values without escaping them, so a
/// reference with a slash or a dot-dot could address a different endpoint. A
/// caller that must refuse a bad reference before touching the daemon, such as
/// the server's container action handler, calls this directly.
///
/// # Errors
///
/// Returns [`DockerError::InvalidContainer`] when the reference is empty,
/// starts with a non-alphanumeric character, exceeds 255 bytes, or contains a
/// character outside `[A-Za-z0-9_.-]`.
pub fn validate_container(container: &str) -> Result<(), DockerError> {
    let mut characters = container.chars();
    let valid = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && container.len() <= 255
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
        });
    if valid {
        Ok(())
    } else {
        Err(DockerError::InvalidContainer {
            name: container.to_owned(),
        })
    }
}

fn stop_timeout(seconds: Option<u32>) -> Result<Option<i32>, DockerError> {
    seconds
        .map(|seconds| i32::try_from(seconds).map_err(|_| DockerError::InvalidTimeout { seconds }))
        .transpose()
}

fn map_error(error: bollard::errors::Error) -> DockerError {
    match error {
        bollard::errors::Error::SocketNotFoundError(path) => DockerError::Unavailable {
            reason: format!("no Docker socket at {path}"),
        },
        bollard::errors::Error::IOError { err } if is_connection_failure(&err) => {
            DockerError::Unavailable {
                reason: err.to_string(),
            }
        }
        other => DockerError::Api(other.to_string()),
    }
}

fn is_connection_failure(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::BrokenPipe
    )
}

#[cfg(test)]
mod tests {
    use super::{Docker, DockerError, log_line, map_error, stop_timeout, validate_container};
    use crate::docker::{ContainerLogLine, LogStream};
    use bollard::container::LogOutput;

    #[test]
    fn plain_names_and_ids_are_accepted() {
        for container in [
            "probe",
            "dandelion-probe-1234",
            "abc123def",
            "my_container.v1",
            "A1",
        ] {
            assert!(
                validate_container(container).is_ok(),
                "{container} should be valid"
            );
        }
    }

    #[test]
    fn path_like_or_empty_references_are_rejected() {
        for container in [
            "",
            "-probe",
            ".probe",
            "../etc/passwd",
            "a/b",
            "probe?force=1",
            "probe name",
            "probe\n",
        ] {
            let err = validate_container(container).expect_err("reference must be rejected");
            assert!(
                matches!(err, DockerError::InvalidContainer { .. }),
                "{container:?} produced {err:?}"
            );
        }
    }

    #[test]
    fn over_long_references_are_rejected() {
        let container = "a".repeat(256);

        assert!(validate_container(&container).is_err());
    }

    #[test]
    fn timeouts_convert_to_docker_integers() {
        assert_eq!(stop_timeout(None).expect("None is valid"), None);
        assert_eq!(stop_timeout(Some(10)).expect("10 is valid"), Some(10));
        assert_eq!(
            stop_timeout(Some(u32::MAX))
                .expect_err("u32::MAX does not fit i32")
                .to_string(),
            format!("timeout {} seconds is out of range", u32::MAX)
        );
    }

    #[test]
    fn a_missing_socket_connect_is_unavailable() {
        let err = Docker::connect_with_socket("/nonexistent/dandelion-probe.sock")
            .expect_err("missing socket must fail to connect");

        assert!(matches!(err, DockerError::Unavailable { .. }));
    }

    #[test]
    fn connection_errors_are_unavailable_but_daemon_errors_are_api_errors() {
        let missing = map_error(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::NotFound, "socket gone"),
        });
        assert!(matches!(missing, DockerError::Unavailable { .. }));

        let refused = map_error(bollard::errors::Error::IOError {
            err: std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused"),
        });
        assert!(matches!(refused, DockerError::Unavailable { .. }));

        let no_socket = map_error(bollard::errors::Error::SocketNotFoundError(
            "/var/run/docker.sock".to_owned(),
        ));
        assert!(matches!(no_socket, DockerError::Unavailable { .. }));

        let daemon = map_error(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            message: "No such container".to_owned(),
        });
        assert!(matches!(daemon, DockerError::Api(message) if message.contains("404")));
    }

    #[test]
    fn log_output_variants_map_to_streams() {
        let stdout = log_line(LogOutput::StdOut {
            message: "hello".into(),
        });
        assert_eq!(
            stdout,
            ContainerLogLine {
                stream: LogStream::Stdout,
                message: "hello".to_owned(),
            }
        );

        let stderr = log_line(LogOutput::StdErr {
            message: "boom".into(),
        });
        assert_eq!(stderr.stream, LogStream::Stderr);
        assert_eq!(stderr.message, "boom");
    }
}
