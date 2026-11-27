//! Documents from declarative plugin manifests.
//!
//! An installed tool drops a JSON manifest that names a collection, a root
//! folder under the owner's home, the include patterns that pick documents out
//! of it, and the grouping order. Dandelion reads manifests as data: it never
//! loads or runs plugin code. Documents are untrusted too; the HTML route
//! serves them in an opaque-origin sandbox and the listing never trusts a
//! path it did not produce itself.

mod listing;
mod manifest;
mod pattern;
mod read;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::manifest::Manifest;

/// Everything the Documents page shows, valid collections and warnings.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentsSnapshot {
    /// Collections from valid manifests, ordered by name.
    pub collections: Vec<Collection>,
    /// Manifest files that were skipped, with the reason.
    pub warnings: Vec<Warning>,
}

/// One collection a manifest declares.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Collection {
    /// Manifest id (the file stem).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Icon name from the manifest's fixed vocabulary.
    pub icon: String,
    /// The root as the manifest wrote it.
    pub root: String,
    /// Documents grouped by the manifest's capture values.
    pub groups: Vec<Group>,
}

/// One group of documents: the capture values and the documents under them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Group {
    /// Capture values in the manifest's `group` order.
    pub values: Vec<String>,
    /// Documents in the group, newest first.
    pub documents: Vec<DocumentEntry>,
}

/// One document in a collection.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentEntry {
    /// Path relative to the collection root, with `/` separators.
    pub path: String,
    /// A readable title derived from the file name.
    pub title: String,
    /// Whether the file is rendered Markdown or sandboxed HTML.
    pub kind: DocumentKind,
    /// Size in bytes.
    pub size: u64,
    /// Modification time as a Unix timestamp in milliseconds.
    pub modified_ms: u64,
}

/// How a document is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DocumentKind {
    /// A self-contained HTML file, served into a sandboxed iframe.
    Html,
    /// GitHub-flavoured Markdown, fetched as text and rendered in the app.
    Markdown,
}

impl DocumentKind {
    /// The kind a file name implies, or `None` for an unsupported extension.
    #[must_use]
    pub fn of_path(path: &str) -> Option<Self> {
        let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "html" | "htm" => Some(Self::Html),
            "md" | "markdown" => Some(Self::Markdown),
            _ => None,
        }
    }

    /// The response content type for this kind.
    #[must_use]
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Html => "text/html; charset=utf-8",
            Self::Markdown => "text/plain; charset=utf-8",
        }
    }
}

/// A manifest file that could not be used.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Warning {
    /// The file name in the plugin folder.
    pub file: String,
    /// Why it was skipped.
    pub problem: String,
}

/// A document's bytes, ready to serve.
#[derive(Debug)]
pub struct DocumentFile {
    /// How the file must be treated.
    pub kind: DocumentKind,
    /// The file contents.
    pub bytes: Vec<u8>,
}

/// Why a document request was refused.
#[derive(Debug, thiserror::Error)]
pub enum DocumentsError {
    /// The collection or the document does not exist, or the path escapes.
    #[error("no such document")]
    NotFound,
    /// The file is larger than the serving cap.
    #[error("the document is too large")]
    TooLarge,
    /// The file could not be read.
    #[error("the document cannot be read")]
    Io(#[from] std::io::Error),
}

/// The plugin folder this server reads manifests from.
#[derive(Debug, Clone)]
pub struct Documents {
    plugins_dir: PathBuf,
    home: PathBuf,
}

impl Documents {
    /// Builds the reader for one plugin folder and one home directory.
    #[must_use]
    pub fn new(plugins_dir: PathBuf, home: PathBuf) -> Self {
        Self { plugins_dir, home }
    }

    /// A reader with no plugin folder: the Documents page is empty.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            plugins_dir: PathBuf::new(),
            home: PathBuf::new(),
        }
    }

    /// The folder manifests are read from.
    #[must_use]
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Lists every valid collection; an unusable manifest becomes a warning.
    ///
    /// # Panics
    ///
    /// Panics never: the read walks the file system and reports failures as
    /// warnings.
    #[must_use]
    pub fn snapshot(&self) -> DocumentsSnapshot {
        let mut collections = Vec::new();
        let mut warnings = Vec::new();

        for (stem, file, text) in listing::manifest_texts(&self.plugins_dir) {
            let parsed = text.and_then(|text| manifest::parse(&text, &stem, &self.home));
            match parsed {
                Ok(manifest) => collections.push(collection(&manifest)),
                Err(problem) => {
                    tracing::warn!(%file, %problem, "skipping a document manifest");
                    warnings.push(Warning { file, problem });
                }
            }
        }
        collections.sort_by(|left, right| left.name.cmp(&right.name));

        DocumentsSnapshot {
            collections,
            warnings,
        }
    }

    /// Reads one document, confined to its collection root.
    ///
    /// # Errors
    ///
    /// `NotFound` for an unknown collection or path, a path outside the root,
    /// a symlink that leaves it, or a file the listing would not produce.
    /// `TooLarge` above the serving cap, and `Io` when the read fails.
    pub fn read(&self, id: &str, path: &str) -> Result<DocumentFile, DocumentsError> {
        let manifest = self.find(id)?;
        read::read_document(&manifest, path)
    }

    /// The manifest for one id, re-read so a changed file is honoured at once.
    fn find(&self, id: &str) -> Result<Manifest, DocumentsError> {
        if !manifest::valid_id(id) {
            return Err(DocumentsError::NotFound);
        }
        let file = self.plugins_dir.join(format!("{id}.json"));
        let text = std::fs::read_to_string(file).map_err(|_| DocumentsError::NotFound)?;
        manifest::parse(&text, id, &self.home).map_err(|_| DocumentsError::NotFound)
    }
}

fn collection(manifest: &Manifest) -> Collection {
    Collection {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        icon: manifest.icon.clone(),
        root: manifest.root_display.clone(),
        groups: listing::list(manifest),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! A home directory and a collection root that tests write into.

    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;
    use tempfile::TempDir;

    pub(crate) struct Fixture {
        pub(crate) _temp: TempDir,
        pub(crate) home: PathBuf,
        pub(crate) root: PathBuf,
    }

    /// A home with a `notes` folder inside it.
    pub(crate) fn fixture() -> Fixture {
        let temp = TempDir::new().expect("a temp dir");
        let home = temp.path().join("home");
        let root = home.join("notes");
        fs::create_dir_all(&root).expect("the fixture tree");
        Fixture {
            _temp: temp,
            home,
            root,
        }
    }

    impl Fixture {
        /// A manifest for this fixture with the given include patterns and no
        /// group names.
        pub(crate) fn manifest_text(&self, include: &[&str]) -> String {
            self.manifest_with_group(&[], include)
        }

        /// A manifest for this fixture with the given group and patterns.
        pub(crate) fn manifest_with_group(&self, group: &[&str], include: &[&str]) -> String {
            json!({
                "schema": 1,
                "id": "probe-visuals",
                "name": "Probe visuals",
                "root": self.root.to_string_lossy(),
                "include": include,
                "group": group,
            })
            .to_string()
        }
    }
}
