//! Search tools — file content pattern matching (grep).
//!
//! Provides `search_for_pattern` to search file contents using regex.

use std::path::Path;

/// Search for a regex pattern in a file and return matching lines.
///
/// Each result contains the line number (1-indexed) and the matching line content.
pub async fn search_for_pattern(
    file_path: &str,
    pattern: &str,
) -> Result<Vec<SearchMatch>, String> {
    let path = Path::new(file_path);
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read file '{file_path}': {e}"))?;

    let re = regex::Regex::new(pattern)
        .map_err(|e| format!("Invalid regex pattern '{pattern}': {e}"))?;

    let mut results = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if re.is_match(line) {
            results.push(SearchMatch {
                line_number: idx + 1,
                content: line.to_string(),
            });
        }
    }

    Ok(results)
}

/// A single match result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchMatch {
    pub line_number: usize,
    pub content: String,
}

/// Register search tools (stub — tools are registered at the MCP level).
pub fn register() {
    tracing::info!("Search tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_search_for_pattern_finds_matches() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world\nfoo bar\nbaz hello\n").unwrap();

        let results = search_for_pattern(
            file_path.to_str().unwrap(),
            "hello",
        ).await.unwrap();

        assert_eq!(results.len(), 2, "should match 2 lines");
        assert_eq!(results[0].line_number, 1);
        assert_eq!(results[0].content, "hello world");
        assert_eq!(results[1].line_number, 3);
        assert_eq!(results[1].content, "baz hello");
    }

    #[tokio::test]
    async fn test_search_for_pattern_no_match() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world\nfoo bar\n").unwrap();

        let results = search_for_pattern(
            file_path.to_str().unwrap(),
            "nonexistent",
        ).await.unwrap();

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_for_pattern_file_not_found() {
        let result = search_for_pattern("/nonexistent/path.txt", "pattern").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot read file"));
    }

    #[tokio::test]
    async fn test_search_for_pattern_invalid_regex() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let result = search_for_pattern(
            file_path.to_str().unwrap(),
            "[invalid",
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid regex pattern"));
    }
}
