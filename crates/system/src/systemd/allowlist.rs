//! The data model for root actions.
//!
//! A system-scoped unit action runs `sudo -n systemctl` and can change the whole
//! machine, so the set of units and verbs it may touch is explicit data. The
//! service refuses anything outside the allowlist before sudo is executed; the
//! sudoers file is the second, independent gate.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::UnitAction;

/// Why a unit name was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameRejection {
    /// The name is empty.
    Empty,
    /// The name is longer than the 255-byte kernel limit.
    TooLong,
    /// The name contains a character that is not valid in a unit name.
    IllegalCharacter(char),
    /// The name starts with `-` or `.`, which would be read as an option or a
    /// hidden file.
    LeadingPunctuation,
}

impl std::fmt::Display for NameRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("the name is empty"),
            Self::TooLong => f.write_str("the name is longer than 255 bytes"),
            Self::IllegalCharacter(character) => {
                write!(f, "`{character}` is not allowed in a unit name")
            }
            Self::LeadingPunctuation => f.write_str("the name must not start with `-` or `.`"),
        }
    }
}

/// A unit name that failed validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid unit name `{name}`: {reason}")]
pub struct UnitNameError {
    /// The rejected name.
    pub name: String,
    /// Why it was rejected.
    pub reason: NameRejection,
}

/// Validates a systemd unit name before it reaches D-Bus or a command line.
///
/// The rules are deliberately narrower than systemd's own: letters, digits and
/// the separators used by real units, no path separators, no whitespace, and no
/// leading punctuation that a tool could mistake for an option.
pub fn validate_unit_name(name: &str) -> Result<(), UnitNameError> {
    let reject = |reason| {
        Err(UnitNameError {
            name: name.to_owned(),
            reason,
        })
    };
    if name.is_empty() {
        return reject(NameRejection::Empty);
    }
    if name.len() > 255 {
        return reject(NameRejection::TooLong);
    }
    if name.starts_with('-') || name.starts_with('.') {
        return reject(NameRejection::LeadingPunctuation);
    }
    for character in name.chars() {
        let allowed = character.is_ascii_alphanumeric()
            || matches!(character, ':' | '_' | '.' | '@' | '-' | '\\');
        if !allowed {
            return reject(NameRejection::IllegalCharacter(character));
        }
    }
    Ok(())
}

/// One allowlist entry: a unit and the verbs permitted on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AllowRule {
    /// Full unit name.
    pub unit: String,
    /// Actions permitted on the unit.
    pub verbs: Vec<UnitAction>,
}

/// A decision against the allowlist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AllowlistError {
    /// The unit name itself is invalid.
    #[error(transparent)]
    InvalidName(#[from] UnitNameError),
    /// The unit has no allowlist entry.
    #[error("unit `{unit}` is not in the sudo allowlist")]
    UnitNotListed {
        /// The unit that was refused.
        unit: String,
    },
    /// The unit is listed, but not for this action.
    #[error("action `{action}` is not allowed for unit `{unit}`")]
    ActionNotAllowed {
        /// The unit that was refused.
        unit: String,
        /// The action that was refused.
        action: UnitAction,
    },
}

/// The units and verbs that may be changed through `sudo -n`.
///
/// Rules are additive: listing the same unit twice allows the union of both
/// verb sets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "RawAllowlist")]
pub struct SudoAllowlist {
    rules: Vec<AllowRule>,
}

/// Deserialization goes through [`SudoAllowlist::allow`], so a loaded
/// configuration cannot smuggle in a unit name that requests could never match.
#[derive(Debug, Deserialize)]
struct RawAllowlist {
    rules: Vec<AllowRule>,
}

impl TryFrom<RawAllowlist> for SudoAllowlist {
    type Error = UnitNameError;

    fn try_from(raw: RawAllowlist) -> Result<Self, Self::Error> {
        let mut allowlist = Self::new();
        for rule in raw.rules {
            allowlist.allow(rule.unit, rule.verbs)?;
        }
        Ok(allowlist)
    }
}

impl SudoAllowlist {
    /// An allowlist that permits nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Validates and adds a rule.
    pub fn allow(
        &mut self,
        unit: impl Into<String>,
        verbs: Vec<UnitAction>,
    ) -> Result<(), UnitNameError> {
        let unit = unit.into();
        validate_unit_name(&unit)?;
        self.rules.push(AllowRule { unit, verbs });
        Ok(())
    }

    /// Returns the configured rules.
    #[must_use]
    pub fn rules(&self) -> &[AllowRule] {
        &self.rules
    }

    /// Whether the allowlist permits nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Decides whether `action` may run on `unit`.
    ///
    /// # Errors
    ///
    /// Returns [`AllowlistError`] when the name is invalid, the unit is not
    /// listed, or the action is not listed for that unit.
    pub fn check(&self, unit: &str, action: UnitAction) -> Result<(), AllowlistError> {
        validate_unit_name(unit)?;
        let mut listed = false;
        for rule in self.rules.iter().filter(|rule| rule.unit == unit) {
            listed = true;
            if rule.verbs.contains(&action) {
                return Ok(());
            }
        }
        if listed {
            Err(AllowlistError::ActionNotAllowed {
                unit: unit.to_owned(),
                action,
            })
        } else {
            Err(AllowlistError::UnitNotListed {
                unit: unit.to_owned(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AllowlistError, NameRejection, SudoAllowlist, UnitNameError, validate_unit_name};
    use crate::systemd::UnitAction;

    fn allowlist() -> SudoAllowlist {
        let mut list = SudoAllowlist::new();
        list.allow("nginx.service", vec![UnitAction::Restart, UnitAction::Stop])
            .expect("nginx rule must be valid");
        list
    }

    #[test]
    fn a_listed_unit_and_verb_are_allowed() {
        let list = allowlist();

        assert!(list.check("nginx.service", UnitAction::Restart).is_ok());
        assert!(list.check("nginx.service", UnitAction::Stop).is_ok());
    }

    #[test]
    fn a_listed_unit_with_an_unlisted_verb_is_refused() {
        let err = allowlist()
            .check("nginx.service", UnitAction::Start)
            .expect_err("start is not allowed for nginx");

        assert_eq!(
            err,
            AllowlistError::ActionNotAllowed {
                unit: "nginx.service".to_owned(),
                action: UnitAction::Start,
            }
        );
    }

    #[test]
    fn an_unlisted_unit_is_refused() {
        let err = allowlist()
            .check("sshd.service", UnitAction::Restart)
            .expect_err("sshd has no rule");

        assert_eq!(
            err,
            AllowlistError::UnitNotListed {
                unit: "sshd.service".to_owned(),
            }
        );
    }

    #[test]
    fn an_empty_allowlist_refuses_everything() {
        let err = SudoAllowlist::new()
            .check("nginx.service", UnitAction::Restart)
            .expect_err("empty allowlist must refuse");

        assert!(matches!(err, AllowlistError::UnitNotListed { .. }));
    }

    #[test]
    fn unit_names_are_matched_exactly_and_case_sensitively() {
        let err = allowlist()
            .check("Nginx.service", UnitAction::Restart)
            .expect_err("matching must be exact");

        assert!(matches!(err, AllowlistError::UnitNotListed { .. }));
    }

    #[test]
    fn rules_are_additive_for_the_same_unit() {
        let mut list = allowlist();
        list.allow("nginx.service", vec![UnitAction::Start])
            .expect("additional rule must be valid");

        assert!(list.check("nginx.service", UnitAction::Start).is_ok());
        assert!(list.check("nginx.service", UnitAction::Restart).is_ok());
    }

    #[test]
    fn unsafe_or_odd_unit_names_are_refused_before_matching() {
        let cases = [
            ("", NameRejection::Empty),
            ("-n", NameRejection::LeadingPunctuation),
            (".hidden.service", NameRejection::LeadingPunctuation),
            ("bad/name.service", NameRejection::IllegalCharacter('/')),
            ("bad name.service", NameRejection::IllegalCharacter(' ')),
            ("bad\nname.service", NameRejection::IllegalCharacter('\n')),
            ("bad;name.service", NameRejection::IllegalCharacter(';')),
            ("bad$(id).service", NameRejection::IllegalCharacter('$')),
        ];
        for (name, reason) in cases {
            let err = validate_unit_name(name).expect_err("name must be rejected");
            assert_eq!(
                err,
                UnitNameError {
                    name: name.to_owned(),
                    reason
                }
            );
        }
    }

    #[test]
    fn a_255_byte_name_is_accepted_and_a_longer_one_is_not() {
        let at_limit = format!("{}.service", "a".repeat(255 - ".service".len()));
        assert!(validate_unit_name(&at_limit).is_ok());

        let over = format!("{}.service", "a".repeat(256 - ".service".len()));
        let err = validate_unit_name(&over).expect_err("over-long name must be rejected");
        assert_eq!(err.reason, NameRejection::TooLong);
    }

    #[test]
    fn adding_a_rule_with_an_invalid_unit_is_refused() {
        let mut list = SudoAllowlist::new();

        let err = list
            .allow("bad/name.service", vec![UnitAction::Restart])
            .expect_err("invalid rule must be refused");

        assert_eq!(err.reason, NameRejection::IllegalCharacter('/'));
        assert!(list.is_empty());
    }

    #[test]
    fn real_world_unit_names_are_accepted() {
        for name in [
            "nginx.service",
            "getty@tty1.service",
            "user@1000.service",
            "systemd-journald.service",
            "dev-disk-by\\x2duuid-1234.device",
            "docker:container.scope",
        ] {
            assert!(validate_unit_name(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn deserializing_a_rule_with_an_invalid_unit_is_refused() {
        let json = r#"{"rules":[{"unit":"bad/name.service","verbs":["restart"]}]}"#;

        let err = serde_json::from_str::<SudoAllowlist>(json)
            .expect_err("a loaded invalid rule must be refused");

        assert!(err.to_string().contains("invalid unit name"));
    }

    #[test]
    fn allowlist_round_trips_through_json() {
        let list = allowlist();

        let json = serde_json::to_string(&list).expect("allowlist must serialize");
        let back: SudoAllowlist = serde_json::from_str(&json).expect("allowlist must parse");

        assert_eq!(back, list);
    }
}
