//! Ignore specification — gitignore pattern matching.
//!
//! Wraps the [`ignore`] crate's gitignore pattern matching to provide
//! a simple `is_ignored` API for project file scanning.

use std::path::Path;

/// A specification of which files/directories to ignore during scanning.
///
/// Wraps the `ignore::Match` infrastructure for gitignore-style
/// pattern matching.
#[derive(Debug, Clone)]
pub struct IgnoreSpec {
    /// The underlying ignore matcher (None = empty spec, nothing ignored).
    matcher: Option<std::sync::Arc<ignore::gitignore::Gitignore>>,
}

impl IgnoreSpec {
    /// Create an empty ignore spec (nothing is ignored).
    pub fn empty() -> Self {
        Self { matcher: None }
    }

    /// Build an `IgnoreSpec` from a `.gitignore` file path.
    ///
    /// Returns an empty spec if the file doesn't exist.
    pub fn from_gitignore<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        if !path.as_ref().exists() {
            return Ok(Self { matcher: None });
        }
        let (gi, err) = ignore::gitignore::Gitignore::new(path.as_ref());
        if let Some(e) = err {
            tracing::warn!(path = %path.as_ref().display(), error = %e, "error loading .gitignore");
        }
        Ok(Self {
            matcher: Some(std::sync::Arc::new(gi)),
        })
    }

    /// Check whether a relative path (or filename) should be ignored.
    ///
    /// `path` should be relative to the project root (e.g., `"target/build.o"`).
    /// Uses `matched_path_or_any_parents` to correctly handle gitignore semantics
    /// where a rule for `target/` applies to everything inside `target/`.
    pub fn is_ignored(&self, path: &str) -> bool {
        match &self.matcher {
            None => false,
            Some(gi) => {
                let p = Path::new(path);
                matches!(
                    gi.matched_path_or_any_parents(p, p.is_dir()),
                    ignore::Match::Ignore(_)
                )
            }
        }
    }
}
