//! Version and protocol policy shared by the backends.

use crate::model::{BackendKind, ProtocolId};

/// The herdr-core handshake generation Dandelion understands.
pub const CORE_GENERATION: u32 = 1;

/// The herdr-core release series this crate was verified against.
pub const CORE_VERSION_SERIES: &str = "0.1";

/// The classic herdr protocol number this crate was verified against.
pub const HERDR_PROTOCOL: u32 = 22;

/// The classic herdr release series this crate was verified against.
pub const HERDR_VERSION_SERIES: &str = "0.9";

/// Features a herdr-core must advertise before writes are allowed.
pub const CORE_REQUIRED_FEATURES: [&str; 4] = ["framed", "attach", "events", "status"];

/// What a backend is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    /// Reads and writes are safe.
    ReadWrite,
    /// The version or protocol is unknown; only reads are allowed.
    ReadOnly,
}

impl Compatibility {
    /// Returns true when writes are allowed.
    #[must_use]
    pub const fn is_read_write(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

/// Decides whether a herdr-core handshake is understood.
///
/// A different generation, a missing version or a version outside the verified
/// `0.1.x` series degrades the backend to read-only.
#[must_use]
pub fn core_compatibility(generation: u32, version: &str) -> Compatibility {
    if generation != CORE_GENERATION {
        return Compatibility::ReadOnly;
    }
    if version_in_series(version, CORE_VERSION_SERIES) {
        Compatibility::ReadWrite
    } else {
        Compatibility::ReadOnly
    }
}

/// Decides whether a classic herdr handshake is understood.
///
/// Protocol 22 with a `0.9.x` version is known; anything else is read-only.
#[must_use]
pub fn classic_compatibility(protocol: u32, version: &str) -> Compatibility {
    if protocol != HERDR_PROTOCOL {
        return Compatibility::ReadOnly;
    }
    if version_in_series(version, HERDR_VERSION_SERIES) {
        Compatibility::ReadWrite
    } else {
        Compatibility::ReadOnly
    }
}

/// Explains why a herdr-core handshake is not fully understood.
///
/// `None` means reads and writes are safe.
#[must_use]
pub fn core_problem(generation: u32, version: &str, features: &[String]) -> Option<String> {
    if generation != CORE_GENERATION {
        return Some(format!(
            "herdr-core generation {generation} is unsupported (expected {CORE_GENERATION})"
        ));
    }
    if !version_in_series(version, CORE_VERSION_SERIES) {
        return Some(read_only_reason(BackendKind::HerdrCore, version));
    }
    for feature in CORE_REQUIRED_FEATURES {
        if !features.iter().any(|advertised| advertised == feature) {
            return Some(format!(
                "herdr-core {version} does not advertise the {feature} feature"
            ));
        }
    }
    None
}

/// Explains why a classic herdr handshake is not fully understood.
///
/// `None` means reads and writes are safe.
#[must_use]
pub fn classic_problem(protocol: u32, version: &str) -> Option<String> {
    if protocol != HERDR_PROTOCOL {
        return Some(format!(
            "herdr protocol {protocol} is unsupported (expected {HERDR_PROTOCOL})"
        ));
    }
    if !version_in_series(version, HERDR_VERSION_SERIES) {
        return Some(read_only_reason(BackendKind::Herdr, version));
    }
    None
}

/// Returns true when `version` is `<series>.<numeric patch>`.
fn version_in_series(version: &str, series: &str) -> bool {
    let Some(rest) = version.trim().strip_prefix(series) else {
        return false;
    };
    let Some(patch) = rest.strip_prefix('.') else {
        return false;
    };
    !patch.is_empty() && patch.bytes().all(|byte| byte.is_ascii_digit())
}

/// Builds the reason shown when a backend is degraded to read-only.
#[must_use]
pub fn read_only_reason(kind: BackendKind, version: &str) -> String {
    match kind {
        BackendKind::HerdrCore => {
            format!("herdr-core {version} is outside the verified {CORE_VERSION_SERIES}.x series")
        }
        BackendKind::Herdr => {
            format!("herdr {version} is outside the verified {HERDR_VERSION_SERIES}.x series")
        }
    }
}

/// The protocol id for a herdr-core generation.
#[must_use]
pub const fn core_protocol(generation: u32) -> ProtocolId {
    ProtocolId::HerdrCore { generation }
}

/// The protocol id for a classic herdr protocol number.
#[must_use]
pub const fn classic_protocol(protocol: u32) -> ProtocolId {
    ProtocolId::Herdr { protocol }
}

#[cfg(test)]
mod tests {
    use super::{
        Compatibility, classic_compatibility, classic_protocol, core_compatibility, core_protocol,
    };
    use crate::model::ProtocolId;

    #[test]
    fn known_core_versions_are_read_write() {
        assert_eq!(core_compatibility(1, "0.1.19"), Compatibility::ReadWrite);
        assert_eq!(core_compatibility(1, "0.1.20"), Compatibility::ReadWrite);
    }

    #[test]
    fn unknown_core_generation_or_version_is_read_only() {
        assert_eq!(core_compatibility(2, "0.1.19"), Compatibility::ReadOnly);
        assert_eq!(core_compatibility(1, "0.2.0"), Compatibility::ReadOnly);
        assert_eq!(core_compatibility(1, ""), Compatibility::ReadOnly);
        assert_eq!(core_compatibility(1, "dev"), Compatibility::ReadOnly);
        assert_eq!(core_compatibility(1, "0.1.x"), Compatibility::ReadOnly);
    }

    #[test]
    fn known_classic_protocol_and_version_are_read_write() {
        assert_eq!(classic_compatibility(22, "0.9.5"), Compatibility::ReadWrite);
        assert_eq!(
            classic_compatibility(22, "0.9.12"),
            Compatibility::ReadWrite
        );
    }

    #[test]
    fn unknown_classic_protocol_or_version_is_read_only() {
        assert_eq!(classic_compatibility(21, "0.9.5"), Compatibility::ReadOnly);
        assert_eq!(classic_compatibility(22, "0.10.0"), Compatibility::ReadOnly);
        assert_eq!(classic_compatibility(22, "0.9"), Compatibility::ReadOnly);
    }

    #[test]
    fn core_problems_name_the_generation_feature_or_version() {
        use super::core_problem;
        let features = ["framed", "attach", "events", "status"].map(str::to_owned);
        assert_eq!(core_problem(1, "0.1.19", &features), None);
        let generation = core_problem(2, "0.1.19", &features).expect("generation problem");
        assert!(generation.contains("generation 2"), "{generation}");
        let version = core_problem(1, "0.2.0", &features).expect("version problem");
        assert!(version.contains("0.2.0"), "{version}");
        let missing = core_problem(1, "0.1.19", &features[..3]).expect("feature problem");
        assert!(missing.contains("status"), "{missing}");
    }

    #[test]
    fn classic_problems_name_the_protocol_or_version() {
        use super::classic_problem;
        assert_eq!(classic_problem(22, "0.9.5"), None);
        let protocol = classic_problem(21, "0.9.5").expect("protocol problem");
        assert!(protocol.contains("protocol 21"), "{protocol}");
        let version = classic_problem(22, "0.10.0").expect("version problem");
        assert!(version.contains("0.10.0"), "{version}");
    }

    #[test]
    fn protocol_ids_name_their_backend() {
        assert_eq!(core_protocol(1), ProtocolId::HerdrCore { generation: 1 });
        assert_eq!(classic_protocol(22), ProtocolId::Herdr { protocol: 22 });
    }
}
