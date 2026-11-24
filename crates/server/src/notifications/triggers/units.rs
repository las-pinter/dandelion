//! systemd units that have just failed.

use std::collections::HashMap;

use dandelion_system::systemd::{UnitInfo, UnitScope};

use super::Triggers;
use crate::notifications::Notice;

impl Triggers {
    /// Checks the unit list for units that have just failed.
    pub fn observe_units(&mut self, units: &[UnitInfo]) -> Vec<Notice> {
        let failed: HashMap<String, &UnitInfo> = units
            .iter()
            .filter(|unit| unit.active_state == "failed")
            .map(|unit| (unit_key(unit), unit))
            .collect();
        let mut notices = Vec::new();
        for (key, unit) in &failed {
            if self.failed_units.insert(key.clone()) {
                let body = match unit.scope {
                    UnitScope::User => format!("User unit {} failed.", unit.name),
                    UnitScope::System => format!("{} failed.", unit.name),
                };
                notices.push(Notice::new(
                    "Service failed",
                    body,
                    format!("unit:{}", unit_key(unit)),
                    "/services",
                ));
            }
        }
        self.failed_units.retain(|key| failed.contains_key(key));
        notices
    }
}

fn unit_key(unit: &UnitInfo) -> String {
    let scope = match unit.scope {
        UnitScope::User => "user",
        UnitScope::System => "system",
    };
    format!("{scope}:{}", unit.name)
}

#[cfg(test)]
mod tests {
    use dandelion_system::systemd::{UnitInfo, UnitScope};

    use super::super::Triggers;

    fn unit(name: &str, state: &str) -> UnitInfo {
        UnitInfo {
            name: name.to_owned(),
            description: String::new(),
            load_state: "loaded".to_owned(),
            active_state: state.to_owned(),
            sub_state: String::new(),
            scope: UnitScope::System,
        }
    }

    #[test]
    fn user_and_system_units_with_one_name_are_distinct() {
        let mut triggers = Triggers::default();
        let mut user = unit("api.service", "failed");
        user.scope = UnitScope::User;

        assert_eq!(triggers.observe_units(&[user.clone()]).len(), 1);
        assert_eq!(
            triggers
                .observe_units(&[user, unit("api.service", "failed")])
                .len(),
            1,
            "the system unit is a different failure"
        );
    }

    #[test]
    fn a_failed_unit_is_announced_once_and_re_armed() {
        let mut triggers = Triggers::default();

        let notices = triggers.observe_units(&[unit("nginx.service", "failed")]);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].body, "nginx.service failed.");
        assert_eq!(
            triggers
                .observe_units(&[unit("nginx.service", "failed")])
                .len(),
            0
        );

        triggers.observe_units(&[unit("nginx.service", "active")]);
        assert_eq!(
            triggers
                .observe_units(&[unit("nginx.service", "failed")])
                .len(),
            1
        );
    }
}
