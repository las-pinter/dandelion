//! Listing the documents a manifest exposes.
//!
//! The walk is bounded: it skips symlinks, never goes deeper than
//! [`MAX_DEPTH`], stops after [`MAX_DOCUMENTS`] files, and ignores a file
//! larger than [`MAX_FILE_BYTES`]. A collection is a view, so a runaway tree
//! shows the first sorted documents rather than stalling the page.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::manifest::Manifest;
use crate::{DocumentEntry, DocumentKind, Group};

/// How deep a collection root may nest.
pub(crate) const MAX_DEPTH: usize = 12;
/// How many documents one collection may list.
pub(crate) const MAX_DOCUMENTS: usize = 2_000;
/// The largest file a document route will serve.
pub(crate) const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// How many manifest files are read at most.
pub(crate) const MAX_MANIFESTS: usize = 64;
/// The largest manifest file a walk will read.
pub(crate) const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Reads every `*.json` manifest in the folder, sorted by file name.
///
/// The text is a `Result` so an oversized or unreadable file becomes a
/// warning row instead of disappearing.
pub(crate) fn manifest_texts(dir: &Path) -> Vec<(String, String, Result<String, String>)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, String, Result<String, String>)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".json")?.to_owned();
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            let text = read_manifest(&entry.path());
            Some((stem, name, text))
        })
        .collect();
    files.sort_by(|left, right| left.1.cmp(&right.1));
    files.truncate(MAX_MANIFESTS);
    files
}

/// Reads one manifest file, refusing one above the size cap.
fn read_manifest(path: &Path) -> Result<String, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("the file cannot be read: {error}"))?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "the file is larger than {} KiB",
            MAX_MANIFEST_BYTES / 1024
        ));
    }
    fs::read_to_string(path).map_err(|error| format!("the file cannot be read: {error}"))
}

/// Lists one collection's documents, grouped as the manifest asks.
pub(crate) fn list(manifest: &Manifest) -> Vec<Group> {
    let mut found: Vec<(Vec<String>, DocumentEntry)> = Vec::new();
    let mut stack = vec![(manifest.root.clone(), 0_usize)];

    while let Some((dir, depth)) = stack.pop() {
        if found.len() >= MAX_DOCUMENTS {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
                continue;
            }
            let Some(relative) = relative_path(&manifest.root, &path) else {
                continue;
            };
            let Some((kind, captures)) = classify(manifest, &relative) else {
                continue;
            };
            let Some(values) = ordered_values(&manifest.group, &captures) else {
                continue;
            };
            found.push((
                values,
                DocumentEntry {
                    title: title_of(&relative),
                    path: relative,
                    kind,
                    size: metadata.len(),
                    modified_ms: modified_ms(&metadata),
                },
            ));
            if found.len() >= MAX_DOCUMENTS {
                break;
            }
        }
    }

    let mut groups: HashMap<Vec<String>, Vec<DocumentEntry>> = HashMap::new();
    for (values, entry) in found {
        groups.entry(values).or_default().push(entry);
    }

    let mut listed: Vec<Group> = groups
        .into_iter()
        .map(|(values, mut documents)| {
            documents.sort_by(|left, right| {
                right
                    .modified_ms
                    .cmp(&left.modified_ms)
                    .then_with(|| left.path.cmp(&right.path))
            });
            Group { values, documents }
        })
        .collect();
    // The group holding the newest document comes first; ties fall back to the
    // group's own values so the order is stable.
    listed.sort_by(|left, right| {
        newest(right)
            .cmp(&newest(left))
            .then_with(|| left.values.cmp(&right.values))
    });
    listed
}

fn newest(group: &Group) -> u64 {
    group
        .documents
        .first()
        .map_or(0, |document| document.modified_ms)
}

/// The path relative to the root with `/` separators, when it is UTF-8.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for part in relative.components() {
        parts.push(part.as_os_str().to_str()?.to_owned());
    }
    Some(parts.join("/"))
}

/// The document kind and captures for a path the manifest accepts.
fn classify(manifest: &Manifest, relative: &str) -> Option<(DocumentKind, Vec<(String, String)>)> {
    let kind = DocumentKind::of_path(relative)?;
    let captures = manifest
        .patterns
        .iter()
        .find_map(|pattern| pattern.captures(relative))?;
    Some((kind, captures))
}

/// The capture values in the manifest's group order, when every name is set.
fn ordered_values(group: &[String], captures: &[(String, String)]) -> Option<Vec<String>> {
    group
        .iter()
        .map(|name| {
            captures
                .iter()
                .find(|(capture, _)| capture == name)
                .map(|(_, value)| value.clone())
        })
        .collect()
}

/// A readable title from the file name: `1611-first-release.html` becomes
/// `First release`.
fn title_of(relative: &str) -> String {
    let stem = relative.rsplit('/').next().unwrap_or(relative);
    let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
    // A leading `HHMM-` is the writer's time; a leading year is not.
    let stem = match stem.split_once('-') {
        Some((prefix, rest))
            if prefix.len() == 4
                && prefix.chars().all(|character| character.is_ascii_digit())
                && rest
                    .split_once('-')
                    .is_none_or(|(second, _)| second.len() != 2) =>
        {
            rest
        }
        _ => stem,
    };
    let words = stem.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Untitled".to_owned(),
    }
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{MAX_DEPTH, list, title_of};
    use crate::manifest::parse;
    use crate::test_support::fixture;

    /// Writes a file with an explicit modification time, so ordering never
    /// depends on how fast the fixture is written.
    fn write_at(path: &std::path::Path, body: &str, unix_seconds: u64) {
        fs::write(path, body).expect("write");
        let file = fs::File::options().write(true).open(path).expect("open");
        file.set_times(
            fs::FileTimes::new()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix_seconds)),
        )
        .expect("set the modification time");
    }

    #[test]
    fn documents_are_grouped_by_the_captures_and_newest_first() {
        let fixture = fixture();
        fs::create_dir_all(fixture.root.join("p/visuals/2026-11-26")).expect("the tree");
        fs::create_dir_all(fixture.root.join("p/visuals/2026-11-27")).expect("the tree");
        write_at(
            &fixture.root.join("p/visuals/2026-11-26/0900-old.md"),
            "# old",
            1_000,
        );
        write_at(
            &fixture.root.join("p/visuals/2026-11-27/1611-new.md"),
            "# new",
            3_000,
        );
        write_at(
            &fixture.root.join("p/visuals/2026-11-27/1700-newer.html"),
            "<p>x</p>",
            2_000,
        );
        fs::write(fixture.root.join("p/visuals/2026-11-27/ignored.txt"), "no").expect("write");
        let manifest = parse(
            &fixture.manifest_with_group(
                &["project", "day"],
                &["{project}/visuals/{day}/**/*.{html,md}"],
            ),
            "probe-visuals",
            &fixture.home,
        )
        .expect("a valid manifest");

        let groups = list(&manifest);

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].values,
            vec!["p".to_owned(), "2026-11-27".to_owned()]
        );
        assert_eq!(groups[0].documents.len(), 2);
        assert!(groups[1].documents.len() == 1, "the older day");
        assert_eq!(groups[1].documents[0].kind, crate::DocumentKind::Markdown);
    }

    #[test]
    fn a_tilde_root_is_walked_from_the_home() {
        let fixture = fixture();
        fs::create_dir_all(fixture.root.join("only")).expect("the tree");
        fs::write(fixture.root.join("only/2026-11-27-1200-a.md"), "# a").expect("write");
        let manifest = parse(
            &fixture.manifest_text(&["**/*.md"]),
            "probe-visuals",
            &fixture.home,
        )
        .expect("a valid manifest");

        let groups = list(&manifest);

        assert_eq!(groups.len(), 1, "no group names means one group");
        assert_eq!(groups[0].values, Vec::<String>::new());
        assert_eq!(groups[0].documents[0].title, "2026 11 27 1200 a");
    }

    #[test]
    fn symlinks_are_skipped() {
        let fixture = fixture();
        fs::create_dir_all(fixture.root.join("d")).expect("the tree");
        fs::write(fixture.root.join("d/real.md"), "# real").expect("write");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            fixture.root.join("d/real.md"),
            fixture.root.join("d/link.md"),
        )
        .expect("symlink");
        let manifest = parse(
            &fixture.manifest_text(&["**/*.md"]),
            "probe-visuals",
            &fixture.home,
        )
        .expect("a valid manifest");

        let groups = list(&manifest);

        assert_eq!(groups[0].documents.len(), 1);
        assert_eq!(groups[0].documents[0].path, "d/real.md");
    }

    #[test]
    fn the_walk_stops_at_its_depth() {
        let fixture = fixture();
        let mut deep = fixture.root.clone();
        for _ in 0..=MAX_DEPTH {
            deep = deep.join("d");
        }
        fs::create_dir_all(&deep).expect("the tree");
        fs::write(deep.join("x.md"), "# deep").expect("write");
        let manifest = parse(
            &fixture.manifest_text(&["**/*.md"]),
            "probe-visuals",
            &fixture.home,
        )
        .expect("a valid manifest");

        let groups = list(&manifest);

        assert!(groups.is_empty(), "a file past the depth cap is not listed");
    }

    #[test]
    fn titles_drop_the_time_prefix_and_separators() {
        assert_eq!(
            title_of("p/visuals/2026-11-27/1611-first-release.html"),
            "First release"
        );
        assert_eq!(title_of("notes/some_file.md"), "Some file");
        assert_eq!(title_of("notes/.md"), "Untitled");
        assert_eq!(title_of("notes/1611-.md"), "Untitled");
    }
}
