//! D-Bus access to the user and system managers.
//!
//! D-Bus is preferred over shelling out to `systemctl` because it returns typed
//! unit data, needs no text parsing, and lets the user manager be reached
//! without a terminal. Only root actions take the command path, because they
//! need sudo's policy check.

use zbus::{Connection, OwnedObjectPath, Type, proxy};

use super::SystemdError;
use super::types::{UnitAction, UnitInfo, UnitScope, UnitStatus};

/// Which bus a manager lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BusKind {
    /// The per-login user bus.
    Session,
    /// The machine-wide system bus.
    System,
}

/// A connection to one systemd manager.
#[derive(Debug, Clone)]
pub(crate) struct Bus {
    connection: Connection,
}

impl Bus {
    /// Connects to the requested bus.
    pub(crate) async fn connect(kind: BusKind) -> Result<Self, SystemdError> {
        let connection = match kind {
            BusKind::Session => Connection::session().await,
            BusKind::System => Connection::system().await,
        }?;
        Ok(Self { connection })
    }

    async fn manager(&self) -> Result<ManagerProxy<'_>, SystemdError> {
        ManagerProxy::new(&self.connection)
            .await
            .map_err(SystemdError::from)
    }

    /// Lists the units known to this manager.
    pub(crate) async fn list_units(&self, scope: UnitScope) -> Result<Vec<UnitInfo>, SystemdError> {
        let units = self.manager().await?.list_units().await?;
        Ok(units
            .into_iter()
            .map(|entry| UnitInfo {
                name: entry.name,
                description: entry.description,
                load_state: entry.load_state,
                active_state: entry.active_state,
                sub_state: entry.sub_state,
                scope,
            })
            .collect())
    }

    /// Reads the unit properties for one unit.
    pub(crate) async fn status(
        &self,
        scope: UnitScope,
        unit: &str,
    ) -> Result<UnitStatus, SystemdError> {
        let path = self.manager().await?.get_unit(unit).await?;
        let proxy = UnitProxy::builder(&self.connection)
            .path(path)
            .build()
            .await?;
        let unit_file_state = proxy
            .unit_file_state()
            .await
            .ok()
            .filter(|state| !state.is_empty());
        let fragment_path = proxy
            .fragment_path()
            .await
            .ok()
            .filter(|path| !path.is_empty());
        Ok(UnitStatus {
            name: proxy.id().await?,
            scope,
            description: proxy.description().await?,
            load_state: proxy.load_state().await?,
            active_state: proxy.active_state().await?,
            sub_state: proxy.sub_state().await?,
            unit_file_state,
            fragment_path,
        })
    }

    /// Runs a mutating action against this manager.
    ///
    /// The manager queues a job and returns immediately; callers that need the
    /// settled state should poll [`Bus::status`].
    pub(crate) async fn apply(&self, action: UnitAction, unit: &str) -> Result<(), SystemdError> {
        let manager = self.manager().await?;
        match action {
            UnitAction::Start => manager.start_unit(unit, "replace").await.map(drop),
            UnitAction::Stop => manager.stop_unit(unit, "replace").await.map(drop),
            UnitAction::Restart => manager.restart_unit(unit, "replace").await.map(drop),
            UnitAction::Enable => manager
                .enable_unit_files(&[unit], false, false)
                .await
                .map(drop),
            UnitAction::Disable => manager.disable_unit_files(&[unit], false).await.map(drop),
        }
        .map_err(SystemdError::from)
    }
}

/// One entry of the `ListUnits` tuple, `a(ssssssouso)`. Every field must stay:
/// they describe the wire tuple, and dropping one changes the signature.
#[derive(Debug, Clone, serde::Deserialize, Type)]
#[allow(dead_code, reason = "the fields mirror the ListUnits reply tuple")]
pub(crate) struct UnitEntry {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub following: String,
    pub object_path: OwnedObjectPath,
    pub job_id: u32,
    pub job_type: String,
    pub job_path: OwnedObjectPath,
}

/// The `EnableUnitFiles` reply, `ba(sss)`.
#[derive(Debug, Clone, serde::Deserialize, Type)]
#[allow(dead_code, reason = "the fields mirror the EnableUnitFiles reply")]
pub(crate) struct EnableUnitFilesResult {
    pub carries_install_info: bool,
    pub changes: Vec<UnitFileChange>,
}

/// One unit-file change, `(sss)`.
#[derive(Debug, Clone, serde::Deserialize, Type)]
#[allow(dead_code, reason = "the fields mirror the DisableUnitFiles reply")]
pub(crate) struct UnitFileChange {
    pub kind: String,
    pub file: String,
    pub source: String,
}

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait Manager {
    fn list_units(&self) -> zbus::Result<Vec<UnitEntry>>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn enable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
        force: bool,
    ) -> zbus::Result<EnableUnitFilesResult>;
    fn disable_unit_files(
        &self,
        files: &[&str],
        runtime: bool,
    ) -> zbus::Result<Vec<UnitFileChange>>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait Unit {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn description(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn unit_file_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn fragment_path(&self) -> zbus::Result<String>;
}
