//! Collection manifests: what a plugin declares and how it is validated.
//!
//! A manifest is data. It names a collection, a root folder under the owner's
//! home, the include patterns that pick documents out of that root, and the
//! grouping order. A file that fails any rule is a warning on the Documents
//! page, never an error for the page as a whole.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::pattern::Pattern;

/// The only manifest schema this build understands.
pub(crate) const SCHEMA: u32 = 1;

const MAX_PATTERNS: usize = 16;
const MAX_GROUP: usize = 3;
const MAX_NAME: usize = 80;

/// Icons a manifest may name; anything else falls back to `document`.
const ICONS: &[&str] = &["document", "chart", "slides", "image", "code"];

/// A validated collection.
#[derive(Debug, Clone)]
pub(crate) struct Manifest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: String,
    /// The root as the manifest wrote it, for display.
    pub(crate) root_display: String,
    /// The root resolved and checked to be inside the home.
    pub(crate) root: PathBuf,
    pub(crate) patterns: Vec<Pattern>,
    pub(crate) group: Vec<String>,
}

/// The manifest file's own shape.
#[derive(Debug, Deserialize)]
struct Raw {
    schema: u32,
    id: String,
    name: String,
    #[serde(default)]
    icon: Option<String>,
    root: String,
    include: Vec<String>,
    #[serde(default)]
    group: Vec<String>,
}

/// Parses and validates one manifest file.
///
/// `stem` is the file name without `.json`; `home` is the owner's home
/// directory from the server's environment.
///
/// # Errors
///
/// Returns the first problem as a message fit for the warning row.
pub(crate) fn parse(text: &str, stem: &str, home: &Path) -> Result<Manifest, String> {
    let raw: Raw =
        serde_json::from_str(text).map_err(|error| format!("not valid JSON: {error}"))?;

    if raw.schema != SCHEMA {
        return Err(format!("unknown schema {}", raw.schema));
    }
    if !valid_id(&raw.id) {
        return Err("id must be 1-64 characters of a-z, 0-9 and -".to_owned());
    }
    if raw.id != stem {
        return Err(format!("id `{}` does not match the file name", raw.id));
    }
    if raw.name.trim().is_empty() || raw.name.chars().count() > MAX_NAME {
        return Err(format!("name must be 1-{MAX_NAME} characters"));
    }
    if raw.include.is_empty() || raw.include.len() > MAX_PATTERNS {
        return Err(format!("include needs 1-{MAX_PATTERNS} patterns"));
    }
    if raw.group.len() > MAX_GROUP {
        return Err(format!("group names at most {MAX_GROUP} levels"));
    }
    for (index, name) in raw.group.iter().enumerate() {
        if !valid_group_name(name) {
            return Err(format!(
                "group name `{name}` must start with a-z and use only a-z, 0-9 and -"
            ));
        }
        if raw.group[..index].contains(name) {
            return Err(format!("group name `{name}` is repeated"));
        }
    }

    let root_text = expand_home(&raw.root, home)
        .ok_or_else(|| "root must start with ~/ or be an absolute path".to_owned())?;
    let root = root_text
        .canonicalize()
        .map_err(|error| format!("root cannot be read: {error}"))?;
    if !root.is_dir() {
        return Err("root is not a directory".to_owned());
    }
    let home = home
        .canonicalize()
        .map_err(|error| format!("home cannot be read: {error}"))?;
    if !root.starts_with(&home) {
        return Err("root must be inside the home directory".to_owned());
    }

    let mut patterns = Vec::with_capacity(raw.include.len());
    for text in &raw.include {
        patterns.push(
            Pattern::parse(text, &raw.group)
                .map_err(|error| format!("include `{text}`: {error}"))?,
        );
    }
    for name in &raw.group {
        let used = raw
            .include
            .iter()
            .any(|text| text.contains(&format!("{{{name}}}")));
        if !used {
            return Err(format!("group name `{name}` appears in no pattern"));
        }
    }

    Ok(Manifest {
        id: raw.id,
        name: raw.name.trim().to_owned(),
        icon: raw
            .icon
            .filter(|icon| ICONS.contains(&icon.as_str()))
            .unwrap_or_else(|| "document".to_owned()),
        root_display: raw.root,
        root,
        patterns,
        group: raw.group,
    })
}

/// Resolves `~` and `~/…` against `home`; other paths must be absolute.
fn expand_home(value: &str, home: &Path) -> Option<PathBuf> {
    if value == "~" {
        return Some(home.to_path_buf());
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return Some(home.join(rest));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return None;
    }
    path.is_absolute().then(|| path.to_path_buf())
}

/// A group name is a capture name: a leading letter, then `[a-z0-9-]`.
pub(crate) fn valid_group_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && value.len() <= 64
}

/// `[a-z0-9]` then up to 63 of `[a-z0-9-]`.
pub(crate) fn valid_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && value.len() <= 64
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use super::{expand_home, parse, valid_id};

    struct Fixture {
        _temp: TempDir,
        home: PathBuf,
        root: PathBuf,
    }

    fn fixture() -> Fixture {
        let temp = TempDir::new().expect("a temp dir");
        let home = temp.path().join("home");
        let root = home.join("notes");
        fs::create_dir_all(root.join("p/visuals/d")).expect("the fixture tree");
        Fixture {
            _temp: temp,
            home,
            root,
        }
    }

    fn manifest(root: &str, group: &[&str], include: &[&str]) -> String {
        serde_json::json!({
            "schema": 1,
            "id": "probe-visuals",
            "name": "Probe visuals",
            "root": root,
            "include": include,
            "group": group,
        })
        .to_string()
    }

    fn root_text(fixture: &Fixture) -> String {
        fixture.root.to_string_lossy().into_owned()
    }

    #[test]
    fn a_manifest_with_a_home_root_and_whole_segment_captures_is_valid() {
        let fixture = fixture();
        let text = manifest(
            &root_text(&fixture),
            &["project", "day"],
            &["{project}/visuals/{day}/**/*.{html,md}"],
        );

        let parsed = parse(&text, "probe-visuals", &fixture.home).expect("a valid manifest");

        assert_eq!(parsed.name, "Probe visuals");
        assert_eq!(parsed.icon, "document");
        assert_eq!(parsed.group, vec!["project".to_owned(), "day".to_owned()]);
        assert!(
            parsed
                .root
                .starts_with(fixture.home.canonicalize().unwrap_or_default())
        );
    }

    #[test]
    fn a_tilde_root_resolves_against_the_home() {
        let fixture = fixture();
        let text = manifest("~/notes", &[], &["**/*.md"]);

        let parsed = parse(&text, "probe-visuals", &fixture.home).expect("a valid manifest");

        assert_eq!(parsed.root_display, "~/notes");
        assert_eq!(
            parsed.root,
            fixture.root.canonicalize().expect("canonical root")
        );
    }

    #[test]
    fn a_root_outside_the_home_is_refused() {
        let fixture = fixture();
        let text = manifest("/etc", &[], &["**/*.md"]);

        let problem =
            parse(&text, "probe-visuals", &fixture.home).expect_err("the fixture is invalid");

        assert!(problem.contains("inside the home"), "{problem}");
    }

    #[test]
    fn a_parent_segment_in_the_root_is_refused() {
        let fixture = fixture();
        let home_text = fixture.home.to_string_lossy();
        let text = manifest(&format!("{home_text}/notes/../.."), &[], &["**/*.md"]);

        let problem =
            parse(&text, "probe-visuals", &fixture.home).expect_err("the fixture is invalid");

        assert!(
            problem.contains("~/") || problem.contains("absolute"),
            "{problem}"
        );
    }

    #[test]
    fn unknown_schema_bad_ids_and_mismatched_stems_are_refused() {
        let fixture = fixture();
        let root = root_text(&fixture);

        let mut schema = manifest(&root, &[], &["**/*.md"]);
        schema = schema.replace("\"schema\":1", "\"schema\":2");
        assert!(
            parse(&schema, "probe-visuals", &fixture.home)
                .expect_err("the fixture is invalid")
                .contains("schema")
        );

        let mut id = manifest(&root, &[], &["**/*.md"]);
        id = id.replace("probe-visuals", "Probe_Visuals");
        assert!(
            parse(&id, "Probe_Visuals", &fixture.home)
                .expect_err("the fixture is invalid")
                .contains("id must be")
        );

        let text = manifest(&root, &[], &["**/*.md"]);
        assert!(
            parse(&text, "other-file", &fixture.home)
                .expect_err("the fixture is invalid")
                .contains("file name")
        );
    }

    #[test]
    fn a_group_name_must_appear_as_a_capture_in_a_pattern() {
        let fixture = fixture();
        let text = manifest(&root_text(&fixture), &["project"], &["**/*.md"]);

        let problem =
            parse(&text, "probe-visuals", &fixture.home).expect_err("the fixture is invalid");

        assert!(problem.contains("appears in no pattern"), "{problem}");
    }

    #[test]
    fn an_unknown_icon_falls_back_to_the_default() {
        let fixture = fixture();
        let text = manifest(&root_text(&fixture), &[], &["**/*.md"]).replace(
            "\"name\":\"Probe visuals\"",
            "\"name\":\"Probe visuals\",\"icon\":\"sparkles\"",
        );

        let parsed = parse(&text, "probe-visuals", &fixture.home).expect("a valid manifest");

        assert_eq!(parsed.icon, "document");
    }

    #[test]
    fn ids_and_home_expansion_follow_the_rules() {
        assert!(valid_id("companion-agents-visuals"));
        assert!(valid_id("a1"));
        assert!(!valid_id(""));
        assert!(!valid_id("Upper"));
        assert!(!valid_id("-leading"));
        assert_eq!(
            expand_home("~/x", Path::new("/home/u")),
            Some(PathBuf::from("/home/u/x"))
        );
        assert_eq!(expand_home("x", Path::new("/home/u")), None);
        assert_eq!(expand_home("/abs/../x", Path::new("/home/u")), None);
    }
}
