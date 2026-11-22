//! Serializable Docker types and typed mutating requests.

use std::collections::BTreeMap;

use bollard::models::{ContainerSummary, ContainerSummaryStateEnum};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Runtime state of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    /// Created but never started.
    Created,
    /// Running.
    Running,
    /// Paused.
    Paused,
    /// Restarting.
    Restarting,
    /// Exited.
    Exited,
    /// Being removed.
    Removing,
    /// Dead.
    Dead,
    /// Stopping.
    Stopping,
    /// A state this crate does not map.
    Unknown,
}

impl ContainerState {
    pub(crate) fn from_summary(state: Option<ContainerSummaryStateEnum>) -> Self {
        match state {
            Some(ContainerSummaryStateEnum::CREATED) => Self::Created,
            Some(ContainerSummaryStateEnum::RUNNING) => Self::Running,
            Some(ContainerSummaryStateEnum::PAUSED) => Self::Paused,
            Some(ContainerSummaryStateEnum::RESTARTING) => Self::Restarting,
            Some(ContainerSummaryStateEnum::EXITED) => Self::Exited,
            Some(ContainerSummaryStateEnum::REMOVING) => Self::Removing,
            Some(ContainerSummaryStateEnum::DEAD) => Self::Dead,
            Some(ContainerSummaryStateEnum::STOPPING) => Self::Stopping,
            Some(ContainerSummaryStateEnum::EMPTY) | None => Self::Unknown,
        }
    }
}

/// A container as listed by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContainerInfo {
    /// Container id.
    pub id: String,
    /// Primary name, without Docker's leading slash.
    pub name: String,
    /// Image reference used to create the container.
    pub image: String,
    /// Command the container runs, when reported.
    pub command: Option<String>,
    /// Runtime state.
    pub state: ContainerState,
    /// Human-readable status line, such as `Up 3 hours`.
    pub status: String,
    /// Creation time as a Unix timestamp in seconds.
    pub created_at_unix: Option<i64>,
    /// Container labels.
    pub labels: BTreeMap<String, String>,
}

impl ContainerInfo {
    pub(crate) fn from_summary(summary: ContainerSummary) -> Self {
        let name = summary
            .names
            .unwrap_or_default()
            .into_iter()
            .next()
            .map_or_else(String::new, |name| name.trim_start_matches('/').to_owned());
        Self {
            id: summary.id.unwrap_or_default(),
            name,
            image: summary.image.unwrap_or_default(),
            command: summary.command,
            state: ContainerState::from_summary(summary.state),
            status: summary.status.unwrap_or_default(),
            created_at_unix: summary.created,
            labels: summary.labels.unwrap_or_default().into_iter().collect(),
        }
    }
}

/// A request to start a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StartContainerRequest {
    /// Container name or id.
    pub container: String,
}

/// A request to stop a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StopContainerRequest {
    /// Container name or id.
    pub container: String,
    /// Seconds to wait before killing the container.
    pub timeout_secs: Option<u32>,
}

/// A request to restart a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RestartContainerRequest {
    /// Container name or id.
    pub container: String,
    /// Seconds to wait before killing the container.
    pub timeout_secs: Option<u32>,
}

/// A request for container logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContainerLogsRequest {
    /// Container name or id.
    pub container: String,
    /// Number of trailing lines to return. `None` returns all buffered lines.
    pub tail: Option<u32>,
    /// Keep the stream open for new lines.
    pub follow: bool,
    /// Prefix each line with its Docker timestamp.
    pub timestamps: bool,
}

/// A request for one statistics sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContainerStatsRequest {
    /// Container name or id.
    pub container: String,
}

/// Which output stream a log line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    /// Standard output.
    Stdout,
    /// Standard error.
    Stderr,
    /// Standard input.
    Stdin,
    /// The container's console.
    Console,
}

/// One log line from a container.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ContainerLogLine {
    /// The stream the line came from.
    pub stream: LogStream,
    /// The line, with invalid UTF-8 replaced.
    pub message: String,
}

/// One statistics sample for a container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct ContainerStats {
    /// The container the sample belongs to.
    pub container: String,
    /// CPU use as a percentage of one core, summed across cores.
    pub cpu_percent: Option<f64>,
    /// Memory in use, in bytes.
    pub memory_usage_bytes: Option<u64>,
    /// Memory limit, in bytes.
    pub memory_limit_bytes: Option<u64>,
    /// Network bytes received, summed over interfaces.
    pub network_rx_bytes: u64,
    /// Network bytes sent, summed over interfaces.
    pub network_tx_bytes: u64,
    /// Number of processes in the container.
    pub pids: Option<u64>,
}

/// Whether the Docker daemon can be reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DockerAvailability {
    /// True when a daemon answered.
    pub available: bool,
    /// Why the daemon is unreachable, when it is not.
    pub reason: Option<String>,
    /// Daemon version, when it answered.
    pub server_version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{ContainerInfo, ContainerState};
    use bollard::models::{ContainerSummary, ContainerSummaryStateEnum};
    use std::collections::HashMap;

    #[test]
    fn summary_names_lose_the_leading_slash() {
        let summary = ContainerSummary {
            id: Some("abc".to_owned()),
            names: Some(vec!["/probe".to_owned(), "/alias".to_owned()]),
            image: Some("alpine:3.21".to_owned()),
            command: Some("sleep 300".to_owned()),
            created: Some(1_700_000_000),
            state: Some(ContainerSummaryStateEnum::RUNNING),
            status: Some("Up 2 minutes".to_owned()),
            labels: Some(HashMap::from([("probe".to_owned(), "yes".to_owned())])),
            ..ContainerSummary::default()
        };

        let info = ContainerInfo::from_summary(summary);

        assert_eq!(info.id, "abc");
        assert_eq!(info.name, "probe");
        assert_eq!(info.image, "alpine:3.21");
        assert_eq!(info.state, ContainerState::Running);
        assert_eq!(info.status, "Up 2 minutes");
        assert_eq!(info.created_at_unix, Some(1_700_000_000));
        assert_eq!(info.labels.get("probe").map(String::as_str), Some("yes"));
    }

    #[test]
    fn a_summary_without_fields_still_maps() {
        let info = ContainerInfo::from_summary(ContainerSummary::default());

        assert_eq!(info.id, "");
        assert_eq!(info.name, "");
        assert_eq!(info.state, ContainerState::Unknown);
        assert!(info.labels.is_empty());
    }

    #[test]
    fn every_docker_state_maps() {
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::CREATED)),
            ContainerState::Created
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::PAUSED)),
            ContainerState::Paused
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::RESTARTING)),
            ContainerState::Restarting
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::EXITED)),
            ContainerState::Exited
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::REMOVING)),
            ContainerState::Removing
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::DEAD)),
            ContainerState::Dead
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::STOPPING)),
            ContainerState::Stopping
        );
        assert_eq!(
            ContainerState::from_summary(Some(ContainerSummaryStateEnum::EMPTY)),
            ContainerState::Unknown
        );
        assert_eq!(ContainerState::from_summary(None), ContainerState::Unknown);
    }
}
