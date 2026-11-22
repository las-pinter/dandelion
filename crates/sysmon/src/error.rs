//! Error type shared by the collector and its parsers.

/// Something went wrong while reading system metrics.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A file under `/proc` or `/sys` could not be read.
    #[error("cannot read {path}: {source}")]
    Io {
        /// Path that could not be read.
        path: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A file under `/proc` or `/sys` had an unexpected shape.
    #[error("malformed metrics data in {path}")]
    Malformed {
        /// Path holding the unexpected data.
        path: String,
    },

    /// The system clock is set before the Unix epoch.
    #[error("system clock is before the Unix epoch")]
    Clock,

    /// The blocking sampling task failed.
    #[error("metrics sampling task failed: {message}")]
    Task {
        /// Reason the task failed.
        message: String,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn malformed(path: impl Into<String>) -> Self {
        Self::Malformed { path: path.into() }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::Error;

    #[test]
    fn io_error_names_the_path_and_keeps_the_source() {
        let error = Error::io(
            "/proc/meminfo",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );

        let shown = error.to_string();

        assert!(shown.contains("/proc/meminfo"), "shown: {shown}");
        assert!(error.source().is_some());
    }

    #[test]
    fn malformed_error_names_the_path() {
        let error = Error::malformed("/proc/stat");

        assert_eq!(error.to_string(), "malformed metrics data in /proc/stat");
    }
}
