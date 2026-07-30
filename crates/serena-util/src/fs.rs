//! Filesystem utilities.

use std::path::Path;

pub fn file_exists(path: &Path) -> bool {
    path.exists()
}
