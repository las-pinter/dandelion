//! Shared domain types used by every Dandelion crate.

/// Build identity of the running binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo {
    pub name: &'static str,
    pub version: &'static str,
}

impl BuildInfo {
    /// Creates build info from a name and version.
    #[must_use]
    pub const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }
}

impl std::fmt::Display for BuildInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::BuildInfo;

    #[test]
    fn display_joins_name_and_version() {
        let info = BuildInfo::new("dandelion", "1.2.3");

        let shown = info.to_string();

        assert_eq!(shown, "dandelion 1.2.3");
    }
}
