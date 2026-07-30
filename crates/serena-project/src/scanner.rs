//! File scanner — walks project files respecting ignore specs.
//!
//! Scans project directories for files matching given extensions,
//! filtering out files/directories matched by the ignore spec.

use std::path::{Path, PathBuf};

use crate::ignore::IgnoreSpec;

/// A file scanner that walks directories respecting ignore rules.
///
/// # Example
///
/// ```rust,ignore
/// use serena_project::scanner::FileScanner;
/// use serena_project::ignore::IgnoreSpec;
///
/// let spec = IgnoreSpec::from_gitignore(".gitignore").unwrap_or_default();
/// let scanner = FileScanner::new(spec);
/// let files = scanner.scan("/path/to/project", &["rs", "py"]).unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct FileScanner {
    ignore: IgnoreSpec,
}

impl FileScanner {
    /// Create a new `FileScanner` with the given ignore spec.
    pub fn new(ignore: IgnoreSpec) -> Self {
        Self { ignore }
    }

    /// Scan a directory and return all files with matching extensions,
    /// excluding files/directories matched by the ignore spec.
    ///
    /// `extensions` should be a list of file extensions without the dot
    /// (e.g., `&["rs", "py"]`).
    pub fn scan<P: AsRef<Path>>(
        &self,
        dir: P,
        extensions: &[&str],
    ) -> std::io::Result<Vec<PathBuf>> {
        let mut results = Vec::new();

        for entry in walkdir::WalkDir::new(dir.as_ref())
            .into_iter()
            .filter_entry(|e| {
                let relative = self.relative_path(dir.as_ref(), e.path());
                !self.ignore.is_ignored(&relative)
            })
        {
            let entry = entry?;
            if entry.file_type().is_dir() {
                continue;
            }

            // Check extension
            if let Some(ext) = entry.path().extension() {
                if extensions.iter().any(|e| ext == *e) {
                    results.push(entry.path().to_path_buf());
                }
            }
        }

        results.sort();
        Ok(results)
    }

    /// Compute a relative path from the scan root.
    fn relative_path(&self, root: &Path, path: &Path) -> String {
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }
}
