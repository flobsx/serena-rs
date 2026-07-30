//! Path manipulation utilities.

pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}
