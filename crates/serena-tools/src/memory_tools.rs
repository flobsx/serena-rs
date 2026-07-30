//! Memory tools — CRUD for persistent memories.
//!
//! Provides tools for writing, reading, listing, and searching
//! persistent memories using the serena-memory backend.

use std::path::PathBuf;
use std::sync::Mutex;

use serena_memory::storage::MemoryStore;

/// Global memory store base path (set once at startup via init_memory_store).
static MEMORY_BASE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Initialise the memory store with a base directory.
/// Called once at server startup.
pub fn init_memory_store(base_dir: &str) {
    let mut guard = MEMORY_BASE.lock().unwrap();
    *guard = Some(PathBuf::from(base_dir));
    tracing::info!(dir = %base_dir, "Memory store initialised");
}

/// Get the memory store, creating with a default path if not initialised.
fn get_store() -> MemoryStore {
    let guard = MEMORY_BASE.lock().unwrap();
    let base = guard.as_ref().cloned().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .join(".serena")
            .join("memories")
    });
    MemoryStore::new(base)
}

/// Write a memory with the given tag and content.
/// Returns the generated memory id.
pub async fn write_memory(tag: &str, content: &str) -> Result<String, String> {
    let store = get_store();
    store.create("default", tag, content)
        .map_err(|e| format!("Failed to write memory: {e}"))
}

/// Read a memory by its id.
pub async fn read_memory(id: &str) -> Result<String, String> {
    let store = get_store();
    store.read("default", id)
        .map_err(|e| format!("Failed to read memory '{id}': {e}"))
}

/// List all memory entries, returning (id, content) pairs.
pub async fn list_memories() -> Result<Vec<(String, String)>, String> {
    let store = get_store();
    store.list("default")
        .map_err(|e| format!("Failed to list memories: {e}"))
}

/// Search memories by tag (empty tag returns all).
pub async fn search_memories(tag: &str) -> Result<Vec<(String, String)>, String> {
    let store = get_store();
    let all = store.list("default")
        .map_err(|e| format!("Failed to search memories: {e}"))?;

    if tag.is_empty() {
        return Ok(all);
    }

    let mut results = Vec::new();
    for (id, _) in &all {
        if let Ok(entry) = store.read_entry("default", id) {
            if entry.tag.contains(tag) {
                results.push((id.clone(), entry.content));
            }
        }
    }
    Ok(results)
}

/// Delete a memory by its id.
pub async fn delete_memory(id: &str) -> Result<(), String> {
    let store = get_store();
    store.delete("default", id)
        .map_err(|e| format!("Failed to delete memory '{id}': {e}"))
}

/// Register memory tools (stub — tools are registered at the MCP level).
pub fn register() {
    tracing::info!("Memory tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use serial_test::serial;

    /// Keep the TempDir alive for the duration of each test.
    fn init_test_store() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join(".serena/memories");
        let mut guard = MEMORY_BASE.lock().unwrap();
        *guard = Some(base);
        dir
    }

    #[tokio::test]
    #[serial]
    async fn test_write_and_read_memory() {
        let _d = init_test_store();
        let id = write_memory("test-tag", "hello world").await.unwrap();
        assert!(!id.is_empty());

        let content = read_memory(&id).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    #[serial]
    async fn test_list_memories() {
        let _d = init_test_store();
        write_memory("tag-a", "content a").await.unwrap();
        write_memory("tag-b", "content b").await.unwrap();

        let list = list_memories().await.unwrap();
        assert_eq!(list.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_memories_by_tag() {
        let _d = init_test_store();
        write_memory("important", "secret data").await.unwrap();
        write_memory("trivial", "junk").await.unwrap();

        let results = search_memories("important").await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "secret data");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_memory() {
        let _d = init_test_store();
        let id = write_memory("temp", "to delete").await.unwrap();

        delete_memory(&id).await.unwrap();

        let result = read_memory(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn test_read_nonexistent() {
        let _d = init_test_store();
        let result = read_memory("nonexistent-id").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn test_search_memories_empty_tag_returns_all() {
        let _d = init_test_store();
        write_memory("a", "content a").await.unwrap();
        write_memory("b", "content b").await.unwrap();

        let results = search_memories("").await.unwrap();
        assert_eq!(results.len(), 2);
    }
}
