//! Live probe for the Docker domain.
//!
//! The test creates its own container with the `docker` CLI, drives it through
//! the library, and removes only that container. It never touches an existing
//! container.
//!
//! Run with `cargo test -p dandelion-system --test docker_live -- --ignored`.

use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dandelion_system::docker::{
    ContainerLogsRequest, ContainerState, ContainerStatsRequest, Docker, RestartContainerRequest,
    StartContainerRequest, StopContainerRequest,
};
use futures_util::StreamExt;
use tokio::time::{sleep, timeout};

fn image() -> String {
    std::env::var("DANDELION_TEST_IMAGE").unwrap_or_else(|_| "alpine:3.21".to_owned())
}

fn unique_container_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after the epoch")
        .as_nanos();
    format!("dandelion-probe-{}-{nanos}", std::process::id())
}

/// Removes the probe container even when an assertion panics.
struct ContainerGuard {
    name: String,
}

impl Drop for ContainerGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "--force", &self.name])
            .status();
    }
}

fn create_container(name: &str) {
    let status = Command::new("docker")
        .args([
            "create",
            "--name",
            name,
            &image(),
            "sh",
            "-c",
            "while true; do echo dandelion-probe-tick; sleep 0.2; done",
        ])
        .status()
        .expect("the docker CLI must run");
    assert!(status.success(), "docker create must succeed");
}

async fn wait_for_running(docker: &Docker, name: &str) {
    for _ in 0..100 {
        let containers = docker
            .list_containers()
            .await
            .expect("container list must load");
        if containers
            .iter()
            .any(|container| container.name == name && container.state == ContainerState::Running)
        {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("container {name} never reached the running state");
}

async fn wait_for_stopped(docker: &Docker, name: &str) {
    for _ in 0..100 {
        let containers = docker
            .list_containers()
            .await
            .expect("container list must load");
        if containers
            .iter()
            .any(|container| container.name == name && container.state == ContainerState::Exited)
        {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("container {name} never reached a stopped state");
}

#[tokio::test]
#[ignore = "creates and removes its own real Docker container"]
async fn probe_container_goes_through_its_full_lifecycle() {
    let docker = Docker::connect_local().expect("Docker must be available for this live probe");
    let availability = docker.availability().await;
    assert!(
        availability.available,
        "Docker daemon must answer: {:?}",
        availability.reason
    );

    let name = unique_container_name();
    let _guard = ContainerGuard { name: name.clone() };
    create_container(&name);

    let listed = docker
        .list_containers()
        .await
        .expect("container list must load");
    let created = listed
        .iter()
        .find(|container| container.name == name)
        .expect("the probe container must be listed");
    assert_eq!(created.state, ContainerState::Created);
    assert_ne!(created.image, "");
    assert!(created.id.len() >= 12);

    docker
        .start(&StartContainerRequest {
            container: name.clone(),
        })
        .await
        .expect("starting the probe container must work");
    wait_for_running(&docker, &name).await;

    let stats = docker
        .stats(&ContainerStatsRequest {
            container: name.clone(),
        })
        .await
        .expect("statistics must load");
    assert_eq!(stats.container, name);
    assert!(
        stats.memory_usage_bytes.is_some_and(|usage| usage > 0),
        "a running container must report memory use"
    );

    let logs = docker
        .logs(&ContainerLogsRequest {
            container: name.clone(),
            tail: Some(100),
            follow: false,
            timestamps: false,
        })
        .expect("logs must open");
    let mut logs = Box::pin(logs);
    let mut seen_marker = false;
    while let Some(item) = logs.next().await {
        let line = item.expect("log lines must stream");
        if line.message.contains("dandelion-probe-tick") {
            seen_marker = true;
            break;
        }
    }
    drop(logs);
    assert!(seen_marker, "the container's own log line must appear");

    let mut following = Box::pin(
        docker
            .logs(&ContainerLogsRequest {
                container: name.clone(),
                tail: Some(0),
                follow: true,
                timestamps: false,
            })
            .expect("following logs must open"),
    );
    let next_line = timeout(Duration::from_secs(10), following.next()).await;
    assert!(
        matches!(next_line, Ok(Some(Ok(_)))),
        "a followed log stream must deliver a line: {next_line:?}"
    );
    drop(following);

    docker
        .restart(&RestartContainerRequest {
            container: name.clone(),
            timeout_secs: Some(10),
        })
        .await
        .expect("restarting the probe container must work");
    wait_for_running(&docker, &name).await;

    docker
        .stop(&StopContainerRequest {
            container: name.clone(),
            timeout_secs: Some(10),
        })
        .await
        .expect("stopping the probe container must work");
    wait_for_stopped(&docker, &name).await;
}
