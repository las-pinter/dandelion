//! systemd units and the journal.
//!
//! Reads and user-unit actions go over D-Bus: typed replies, no text parsing,
//! and no terminal needed. System-unit actions are the one exception. They
//! change the whole machine, so they go through `sudo -n systemctl` after an
//! explicit allowlist decision.
//!
//! The user manager is reached on the session bus and the system manager on the
//! system bus. Both connections are opened by [`Systemd::connect`].

pub mod allowlist;
mod dbus;
mod journal;
mod sudo;
mod types;

pub use allowlist::{
    AllowRule, AllowlistError, NameRejection, SudoAllowlist, UnitNameError, validate_unit_name,
};
pub use journal::{JournalError, JournalStream};
pub use sudo::SudoError;
pub use types::{
    ActionMechanism, JournalEntry, JournalFollowRequest, JournalTailRequest, UnitAction,
    UnitActionRequest, UnitActionResult, UnitInfo, UnitScope, UnitStatus,
};

use dbus::{Bus, BusKind};

/// Errors from the systemd domain.
#[derive(Debug, thiserror::Error)]
pub enum SystemdError {
    /// A local file or child process failed.
    #[error("systemd I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// A D-Bus call failed, including "unit not found" replies.
    #[error("systemd D-Bus error: {0}")]
    Bus(#[from] zbus::Error),
    /// A unit name failed validation.
    #[error(transparent)]
    UnitName(#[from] UnitNameError),
    /// The sudo allowlist refused the action.
    #[error(transparent)]
    Allowlist(#[from] AllowlistError),
    /// The sudo path failed.
    #[error(transparent)]
    Sudo(#[from] SudoError),
    /// The journal could not be read.
    #[error(transparent)]
    Journal(#[from] JournalError),
}

/// The systemd service: both manager connections plus the root-action policy.
#[derive(Debug)]
pub struct Systemd {
    user: Bus,
    system: Bus,
    allowlist: SudoAllowlist,
}

impl Systemd {
    /// Connects to the user and system managers.
    ///
    /// # Errors
    ///
    /// Returns an error when either bus is unreachable, such as in a container
    /// without a systemd user manager.
    pub async fn connect(allowlist: SudoAllowlist) -> Result<Self, SystemdError> {
        let user = Bus::connect(BusKind::Session).await?;
        let system = Bus::connect(BusKind::System).await?;
        Ok(Self {
            user,
            system,
            allowlist,
        })
    }

    /// The root-action allowlist in force.
    #[must_use]
    pub fn allowlist(&self) -> &SudoAllowlist {
        &self.allowlist
    }

    fn bus(&self, scope: UnitScope) -> &Bus {
        match scope {
            UnitScope::User => &self.user,
            UnitScope::System => &self.system,
        }
    }

    /// Lists the units owned by one manager.
    pub async fn list_units(&self, scope: UnitScope) -> Result<Vec<UnitInfo>, SystemdError> {
        self.bus(scope).list_units(scope).await
    }

    /// Reads one unit's status.
    pub async fn status(&self, scope: UnitScope, unit: &str) -> Result<UnitStatus, SystemdError> {
        validate_unit_name(unit)?;
        self.bus(scope).status(scope, unit).await
    }

    /// Runs a mutating action on a unit.
    ///
    /// A user unit goes straight to the user manager over D-Bus. A system unit
    /// must pass the allowlist first and then runs through `sudo -n`; an
    /// unlisted unit is refused without sudo ever being started.
    pub async fn act(&self, request: &UnitActionRequest) -> Result<UnitActionResult, SystemdError> {
        validate_unit_name(&request.unit)?;
        let mechanism = match request.scope {
            UnitScope::User => {
                self.user.apply(request.action, &request.unit).await?;
                ActionMechanism::Dbus
            }
            UnitScope::System => {
                self.allowlist.check(&request.unit, request.action)?;
                sudo::run(request.action, &request.unit).await?;
                ActionMechanism::Sudo
            }
        };
        Ok(UnitActionResult {
            scope: request.scope,
            unit: request.unit.clone(),
            action: request.action,
            mechanism,
        })
    }

    /// Reads the latest journal records for a unit.
    pub async fn journal_tail(
        &self,
        request: &JournalTailRequest,
    ) -> Result<Vec<JournalEntry>, SystemdError> {
        journal::tail(request).await
    }

    /// Follows a unit's journal until the returned stream is dropped.
    pub fn journal_follow(
        &self,
        request: &JournalFollowRequest,
    ) -> Result<JournalStream, SystemdError> {
        journal::follow(request)
    }
}
