//! Memory storage backend — reads/writes memory files as markdown.
//!
//! Each memory is stored as an individual markdown file under
//! `.serena/memories/{namespace}/{id}.md`. The file contains the
//! memory content plus frontmatter metadata (key, tag, created_at).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

/// A persistent memory stored as a markdown file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier (filename stem).
    pub id: String,
    /// Semantic tag/key for categorisation.
    pub tag: String,
    /// The memory content (markdown body).
    pub content: String,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 last-updated timestamp.
    pub updated_at: String,
}

/// Backend storage for persistent memories.
///
/// Reads and writes memory files as markdown under a configurable base
/// directory. Each namespace (e.g., `"project"`, `"global"`) gets its
/// own subdirectory.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    /// Base directory for memory storage (e.g., `.serena/memories`).
    base: PathBuf,
}

impl MemoryStore {
    /// Create a new `MemoryStore` rooted at the given directory.
    ///
    /// The directory is created if it doesn't exist.
    pub fn new<P: AsRef<Path>>(base: P) -> Self {
        let base = base.as_ref().to_path_buf();
        let _ = fs::create_dir_all(&base);
        Self { base }
    }

    /// Namespace directory path.
    fn namespace_dir(&self, namespace: &str) -> PathBuf {
        self.base.join(namespace)
    }

    /// File path for a specific memory by id.
    fn memory_path(&self, namespace: &str, id: &str) -> PathBuf {
        self.namespace_dir(namespace).join(format!("{id}.md"))
    }

    /// Generate a unique memory id based on tag and timestamp.
    fn generate_id(&self, tag: &str) -> String {
        let ts = Utc::now().format("%Y%m%d%H%M%S%3f");
        // Sanitise tag for use in filenames
        let safe_tag: String = tag.chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        format!("{}_{}", safe_tag, ts)
    }

    /// Create a new memory entry.
    ///
    /// Returns the generated id.
    pub fn create(&self, namespace: &str, tag: &str, content: &str) -> std::io::Result<String> {
        let ns_dir = self.namespace_dir(namespace);
        fs::create_dir_all(&ns_dir)?;

        let id = self.generate_id(tag);
        let now = Utc::now().to_rfc3339();

        let entry = MemoryEntry {
            id: id.clone(),
            tag: tag.to_string(),
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        let path = self.memory_path(namespace, &id);
        let serialised = serde_yaml::to_string(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let markdown = format!("---\n{serialised}---\n\n{}", entry.content);
        fs::write(&path, markdown)?;

        Ok(id)
    }

    /// Read a memory entry by id.
    pub fn read(&self, namespace: &str, id: &str) -> std::io::Result<String> {
        let path = self.memory_path(namespace, id);
        let raw = fs::read_to_string(&path)?;

        // Parse frontmatter to extract content
        Self::parse_memory_file(&raw)
            .map(|entry| entry.content)
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("failed to parse memory file: {}", path.display()),
            ))
    }

    /// Read the full `MemoryEntry` for a given id.
    pub fn read_entry(&self, namespace: &str, id: &str) -> std::io::Result<MemoryEntry> {
        let path = self.memory_path(namespace, id);
        let raw = fs::read_to_string(&path)?;

        Self::parse_memory_file(&raw).ok_or_else(|| std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to parse memory file: {}", path.display()),
        ))
    }

    /// Update an existing memory entry's content.
    pub fn update(&self, namespace: &str, id: &str, content: &str) -> std::io::Result<()> {
        let path = self.memory_path(namespace, id);
        let raw = fs::read_to_string(&path)?;

        let mut entry = Self::parse_memory_file(&raw).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed memory file")
        })?;

        entry.content = content.to_string();
        entry.updated_at = Utc::now().to_rfc3339();

        let serialised = serde_yaml::to_string(&entry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let markdown = format!("---\n{serialised}---\n\n{}", entry.content);
        fs::write(&path, markdown)?;

        Ok(())
    }

    /// Delete a memory entry by id.
    pub fn delete(&self, namespace: &str, id: &str) -> std::io::Result<()> {
        let path = self.memory_path(namespace, id);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// List all memory entry ids and their content in a namespace.
    pub fn list(&self, namespace: &str) -> std::io::Result<Vec<(String, String)>> {
        let ns_dir = self.namespace_dir(namespace);
        if !ns_dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in fs::read_dir(&ns_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "md") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(raw) = fs::read_to_string(&path) {
                        if let Some(entry) = Self::parse_memory_file(&raw) {
                            results.push((stem.to_string(), entry.content));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(results)
    }

    /// Parse a markdown file with YAML frontmatter into a `MemoryEntry`.
    fn parse_memory_file(raw: &str) -> Option<MemoryEntry> {
        let raw_trimmed = raw.trim();
        if !raw_trimmed.starts_with("---") {
            return None;
        }

        // Find the closing ---
        let rest = &raw_trimmed[3..];
        let end = rest.find("\n---")?;
        let yaml_str = &rest[..end];

        let mut entry: MemoryEntry = serde_yaml::from_str(yaml_str).ok()?;

        // The content after the frontmatter
        let body_start = end + 5; // "\n---" + maybe whitespace
        let body = rest[body_start..].trim();
        if !body.is_empty() {
            entry.content = body.to_string();
        }

        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creates_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join(".serena/memories"));
        assert!(store.base.exists());
    }

    #[test]
    fn test_store_list_empty_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::new(dir.path().join(".serena/memories"));
        let items = store.list("nonexistent").unwrap();
        assert!(items.is_empty());
    }
}
