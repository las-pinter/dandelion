//! Reading one document, confined to its collection root.
//!
//! Three checks stand between a request path and a file: the path must be
//! relative, match one of the manifest's patterns, and resolve through real
//! directories inside the root. Any symlink on the way is refused, and the
//! canonical path is checked against the root again before anything is read.

use std::fs;
use std::path::{Path, PathBuf};

use crate::listing::{MAX_DEPTH, MAX_FILE_BYTES};
use crate::manifest::Manifest;
use crate::{DocumentFile, DocumentKind, DocumentsError};

/// Reads one document of an already-resolved manifest.
pub(crate) fn read_document(
    manifest: &Manifest,
    path: &str,
) -> Result<DocumentFile, DocumentsError> {
    let segments = relative_segments(path).ok_or(DocumentsError::NotFound)?;
    if segments.len() > MAX_DEPTH + 1 {
        return Err(DocumentsError::NotFound);
    }
    if manifest
        .patterns
        .iter()
        .all(|pattern| pattern.captures(path).is_none())
    {
        return Err(DocumentsError::NotFound);
    }
    let kind = DocumentKind::of_path(path).ok_or(DocumentsError::NotFound)?;

    let file = resolve(&manifest.root, &segments)?;
    let metadata = fs::metadata(&file).map_err(|_| DocumentsError::NotFound)?;
    if !metadata.is_file() {
        return Err(DocumentsError::NotFound);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(DocumentsError::TooLarge);
    }

    Ok(DocumentFile {
        kind,
        bytes: fs::read(&file)?,
    })
}

/// The path's segments, or `None` when the path is absolute or contains an
/// empty, `.` or `..` segment.
fn relative_segments(path: &str) -> Option<Vec<&str>> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return None;
    }
    let segments: Vec<&str> = path.split('/').collect();
    segments
        .iter()
        .all(|segment| !segment.is_empty() && *segment != "." && *segment != "..")
        .then_some(segments)
}

/// Walks the segments without following a symlink, then confirms the
/// canonical file still sits under the root.
fn resolve(root: &Path, segments: &[&str]) -> Result<PathBuf, DocumentsError> {
    let mut current = root.to_path_buf();
    for segment in segments {
        current.push(segment);
        let metadata = fs::symlink_metadata(&current).map_err(|_| DocumentsError::NotFound)?;
        if metadata.file_type().is_symlink() {
            return Err(DocumentsError::NotFound);
        }
    }

    let canonical = current
        .canonicalize()
        .map_err(|_| DocumentsError::NotFound)?;
    if !canonical.starts_with(root) {
        return Err(DocumentsError::NotFound);
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::test_support::{Fixture, fixture};
    use crate::{DocumentKind, Documents, DocumentsError};

    fn documents(fixture: &Fixture) -> Documents {
        let plugins = fixture.home.join("plugins");
        fs::create_dir_all(&plugins).expect("the plugin folder");
        fs::write(
            plugins.join("probe-visuals.json"),
            fixture.manifest_text(&["**/*.{html,md}"]),
        )
        .expect("the manifest");
        Documents::new(plugins, fixture.home.clone())
    }

    fn write(fixture: &Fixture, relative: &str, body: &str) {
        let path = fixture.root.join(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("the tree");
        fs::write(path, body).expect("the file");
    }

    #[test]
    fn a_listed_document_is_served_with_its_kind() {
        let fixture = fixture();
        write(&fixture, "d/one-pager.html", "<!doctype html><p>hi</p>");
        let documents = documents(&fixture);

        let file = documents
            .read("probe-visuals", "d/one-pager.html")
            .expect("the document");

        assert_eq!(file.kind, DocumentKind::Html);
        assert_eq!(file.bytes, b"<!doctype html><p>hi</p>");
    }

    #[test]
    fn a_markdown_document_is_text() {
        let fixture = fixture();
        write(&fixture, "d/notes.md", "# title");
        let documents = documents(&fixture);

        let file = documents
            .read("probe-visuals", "d/notes.md")
            .expect("the document");

        assert_eq!(file.kind, DocumentKind::Markdown);
        assert_eq!(file.kind.content_type(), "text/plain; charset=utf-8");
    }

    #[test]
    fn parent_segments_absolute_paths_and_non_matching_files_are_refused() {
        let fixture = fixture();
        write(&fixture, "d/x.md", "# x");
        fs::write(fixture.home.join("secret.md"), "secret").expect("the secret");
        let documents = documents(&fixture);

        for path in ["../home/secret.md", "/etc/passwd", "d/../x.md", "d/x.txt"] {
            let result = documents.read("probe-visuals", path);
            assert!(
                matches!(result, Err(DocumentsError::NotFound)),
                "{path} must be refused"
            );
        }
    }

    #[test]
    fn a_symlink_is_refused_even_when_it_stays_inside_the_root() {
        let fixture = fixture();
        write(&fixture, "d/real.md", "# real");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            fixture.root.join("d/real.md"),
            fixture.root.join("d/link.md"),
        )
        .expect("the symlink");
        let documents = documents(&fixture);

        let result = documents.read("probe-visuals", "d/link.md");

        assert!(matches!(result, Err(DocumentsError::NotFound)));
        assert!(documents.read("probe-visuals", "d/real.md").is_ok());
    }

    #[test]
    fn an_oversized_document_is_refused() {
        let fixture = fixture();
        write(&fixture, "big.md", "");
        let path = fixture.root.join("big.md");
        let file = fs::File::create(&path).expect("the file");
        file.set_len(8 * 1024 * 1024 + 1).expect("a sparse file");
        let documents = documents(&fixture);

        let result = documents.read("probe-visuals", "big.md");

        assert!(matches!(result, Err(DocumentsError::TooLarge)));
    }

    #[test]
    fn an_unknown_or_misspelled_collection_is_refused() {
        let fixture = fixture();
        let documents = documents(&fixture);

        assert!(matches!(
            documents.read("nope", "d/x.md"),
            Err(DocumentsError::NotFound)
        ));
        assert!(matches!(
            documents.read("../plugins/probe-visuals", "d/x.md"),
            Err(DocumentsError::NotFound)
        ));
    }

    #[test]
    fn the_snapshot_lists_valid_collections_and_warns_about_broken_manifests() {
        let fixture = fixture();
        write(&fixture, "d/one-pager.html", "<p>hi</p>");
        let documents = documents(&fixture);
        fs::write(documents.plugins_dir().join("broken.json"), "{ not json").expect("a bad file");

        let snapshot = documents.snapshot();

        assert_eq!(snapshot.collections.len(), 1);
        assert_eq!(snapshot.collections[0].id, "probe-visuals");
        assert_eq!(snapshot.collections[0].root, fixture.root.to_string_lossy());
        assert_eq!(snapshot.collections[0].groups.len(), 1);
        assert_eq!(snapshot.warnings.len(), 1);
        assert_eq!(snapshot.warnings[0].file, "broken.json");
    }

    #[test]
    fn a_manifest_without_a_group_lists_one_ungrouped_collection() {
        let fixture = fixture();
        write(&fixture, "d/notes.md", "# notes");
        let plugins = fixture.home.join("plugins");
        fs::create_dir_all(&plugins).expect("the plugin folder");
        let text = format!(
            r#"{{"schema":1,"id":"probe-visuals","name":"Probe visuals","root":{},"include":["**/*.md"]}}"#,
            serde_json::to_string(&fixture.root.to_string_lossy()).expect("a JSON string")
        );
        fs::write(plugins.join("probe-visuals.json"), text).expect("the manifest");

        let snapshot = Documents::new(plugins, fixture.home.clone()).snapshot();

        assert!(snapshot.warnings.is_empty(), "{:?}", snapshot.warnings);
        assert_eq!(
            snapshot.collections[0].groups[0].values,
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_oversized_manifest_is_a_warning() {
        let fixture = fixture();
        let plugins = fixture.home.join("plugins");
        fs::create_dir_all(&plugins).expect("the plugin folder");
        let text =
            "x".repeat(usize::try_from(crate::listing::MAX_MANIFEST_BYTES).expect("fits") + 1);
        fs::write(plugins.join("huge.json"), text).expect("the manifest");

        let snapshot = Documents::new(plugins, fixture.home.clone()).snapshot();

        assert!(snapshot.collections.is_empty());
        assert_eq!(snapshot.warnings.len(), 1);
        assert!(snapshot.warnings[0].problem.contains("larger than"));
    }

    #[test]
    fn a_missing_plugin_folder_is_an_empty_page() {
        let fixture = fixture();

        let snapshot = Documents::new(fixture.home.join("absent"), fixture.home.clone()).snapshot();

        assert!(snapshot.collections.is_empty());
        assert!(snapshot.warnings.is_empty());
    }

    #[test]
    fn a_manifest_that_names_an_unknown_pattern_language_is_a_warning() {
        let fixture = fixture();
        let plugins = fixture.home.join("plugins");
        fs::create_dir_all(&plugins).expect("the plugin folder");
        // A capture that group never lists is invalid.
        let text = fixture.manifest_text(&["{project}/**/*.md"]);
        fs::write(plugins.join("probe-visuals.json"), text).expect("the manifest");

        let snapshot = Documents::new(plugins, fixture.home.clone()).snapshot();

        assert!(snapshot.collections.is_empty());
        assert_eq!(snapshot.warnings.len(), 1);
    }
}
