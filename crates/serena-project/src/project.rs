//! Project management — root detection, project config, state.
//!
//! The `Project` struct represents a Serena-enabled directory with
//! its root path, ignore rules, and project config.

use std::path::{Path, PathBuf};

/// Represents an open Serena project.
///
/// A project is identified by its root directory and provides access
/// to ignore specs, file scanning, and project-level configuration.
#[derive(Debug, Clone)]
pub struct Project {
    /// Absolute path to the project root directory.
    root: PathBuf,
}

impl Project {
    /// Open a Serena project at the given path.
    ///
    /// This resolves the root directory, canonicalizes the path,
    /// and detects project-level config. Returns an error if the
    /// path doesn't exist or isn't a directory.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the path doesn't exist or can't be
    /// canonicalized.
    pub fn open<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let canonical = path.as_ref().canonicalize()?;
        if !canonical.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("path is not a directory: {}", canonical.display()),
            ));
        }
        Ok(Self { root: canonical })
    }

    /// Return the project root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a new Project directly from a root path (without validation).
    /// Used internally and in tests.
    pub fn from_root(root: PathBuf) -> Self {
        Self { root }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_project_open_and_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.txt"), "hello").unwrap();

        let project = Project::open(dir.path().to_str().unwrap()).unwrap();
        assert_eq!(project.root(), dir.path().canonicalize().unwrap());
    }

    #[test]
    fn test_project_open_nonexistent() {
        let result = Project::open("/tmp/serena-nonexistent-dir-12345");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_open_file_instead_of_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = Project::open(&file_path);
        assert!(result.is_err());
    }
}
