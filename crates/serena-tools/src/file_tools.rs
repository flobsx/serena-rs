//! File tools — read, list, find, and replace file contents.
//!
//! Provides tools for basic filesystem operations: reading files,
//! listing directories, finding files by pattern, and replacing
//! content within files.

use std::path::Path;

/// Read the entire contents of a file as a string.
pub async fn read_file(file_path: &str) -> Result<String, String> {
    let path = Path::new(file_path);
    std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read file '{file_path}': {e}"))
}

/// List all entries (files + directories) in a directory.
pub async fn list_dir(dir_path: &str) -> Result<Vec<DirEntry>, String> {
    let path = Path::new(dir_path);
    let read_dir = std::fs::read_dir(path)
        .map_err(|e| format!("Cannot list directory '{dir_path}': {e}"))?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Error reading directory entry: {e}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|e| format!("Error reading file type: {e}"))?;

        entries.push(DirEntry {
            name,
            is_dir: file_type.is_dir(),
            is_file: file_type.is_file(),
            is_symlink: file_type.is_symlink(),
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Find files matching a glob-like pattern in a directory tree.
///
/// Simple substring matching (e.g., "*.rs" matches files ending in ".rs").
pub async fn find_file(dir_path: &str, pattern: &str) -> Result<Vec<String>, String> {
    let path = Path::new(dir_path);
    if !path.is_dir() {
        return Err(format!("Not a directory: '{dir_path}'"));
    }

    let pattern: Box<dyn Fn(&str) -> bool> = if pattern.starts_with('*') {
        // Convert "*.rs" to suffix match
        let suffix = pattern[1..].to_string();
        Box::new(move |name: &str| name.ends_with(&suffix))
    } else if pattern.ends_with('*') {
        let prefix = pattern[..pattern.len() - 1].to_string();
        Box::new(move |name: &str| name.starts_with(&prefix))
    } else {
        let p = pattern.to_string();
        Box::new(move |name: &str| name.contains(&p))
    };

    let mut results = Vec::new();
    walk_tree(path, &mut results, &pattern)?;
    Ok(results)
}

fn walk_tree<F>(dir: &Path, results: &mut Vec<String>, matcher: &F) -> Result<(), String>
where
    F: Fn(&str) -> bool,
{
    let read_dir = std::fs::read_dir(dir)
        .map_err(|e| format!("Cannot read directory '{}': {e}", dir.display()))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Error reading entry: {e})))"))?;
        let name = entry.file_name().to_string_lossy().to_string();

        if matcher(&name) {
            results.push(entry.path().to_string_lossy().to_string());
        }

        let file_type = entry.file_type().map_err(|e| format!("Error reading file type: {e}"))?;
        if file_type.is_dir() {
            walk_tree(&entry.path(), results, matcher)?;
        }
    }

    Ok(())
}

/// Search and replace within a file content.
/// Returns the number of replacements made.
pub async fn replace_content(file_path: &str, search: &str, replacement: &str) -> Result<usize, String> {
    let path = Path::new(file_path);
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read file '{file_path}': {e}"))?;

    // Simple string replace (not regex)
    let new_content = content.replace(search, replacement);
    let count = content.matches(search).count();

    if count > 0 {
        std::fs::write(path, &new_content)
            .map_err(|e| format!("Cannot write file '{file_path}': {e}"))?;
    }

    Ok(count)
}

/// A directory entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
}

/// Register file tools (stub — tools are registered at the MCP level).
pub fn register() {
    tracing::info!("File tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // -----------------------------------------------------------------------
    // read_file tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_read_file_success() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world\n").unwrap();

        let content = read_file(file_path.to_str().unwrap()).await.unwrap();
        assert_eq!(content, "hello world\n");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let result = read_file("/nonexistent/path.txt").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read file"));
    }

    // -----------------------------------------------------------------------
    // list_dir tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_dir_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let entries = list_dir(dir.path().to_str().unwrap()).await.unwrap();
        assert_eq!(entries.len(), 3);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));
        assert!(names.contains(&"sub"));

        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert!(sub.is_dir);
        assert!(!sub.is_file);
    }

    #[tokio::test]
    async fn test_list_dir_not_found() {
        let result = list_dir("/nonexistent/dir").await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // find_file tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_find_file_by_suffix() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "").unwrap();
        fs::write(dir.path().join("lib.rs"), "").unwrap();
        fs::write(dir.path().join("README.md"), "").unwrap();

        let results = find_file(dir.path().to_str().unwrap(), "*.rs").await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|p| p.ends_with(".rs")));
    }

    #[tokio::test]
    async fn test_find_file_recursive() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("root.txt"), "").unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.txt"), "").unwrap();

        let results = find_file(dir.path().to_str().unwrap(), "*.txt").await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_find_file_not_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "").unwrap();

        let result = find_file(file_path.to_str().unwrap(), "*.rs").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not a directory"));
    }

    // -----------------------------------------------------------------------
    // replace_content tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_replace_content_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz").unwrap();

        let count = replace_content(
            file_path.to_str().unwrap(),
            "foo",
            "qux",
        ).await.unwrap();

        assert_eq!(count, 2, "should replace 2 occurrences");
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "qux bar qux baz");
    }

    #[tokio::test]
    async fn test_replace_content_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let count = replace_content(
            file_path.to_str().unwrap(),
            "nonexistent",
            "replaced",
        ).await.unwrap();

        assert_eq!(count, 0);
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn test_replace_content_file_not_found() {
        let result = replace_content("/nonexistent.txt", "foo", "bar").await;
        assert!(result.is_err());
    }
}
