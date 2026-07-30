//! Memory manager — persistent memory storage and retrieval.
//!
//! Provides a high-level CRUD API over the `MemoryStore` backend,
//! with convenience methods for finding memories by tag and
//! managing project vs global memory scopes.

use crate::storage::MemoryStore;

/// High-level memory manager.
///
/// Wraps a `MemoryStore` with convenience operations for setting,
/// getting, finding, and deleting memories.
#[derive(Debug, Clone)]
pub struct MemoryManager {
    store: MemoryStore,
}

impl MemoryManager {
    /// Create a new `MemoryManager` backed by the given store.
    pub fn new(store: MemoryStore) -> Self {
        Self { store }
    }

    /// Store a memory in the given namespace with a tag and content.
    ///
    /// Returns the generated unique id.
    pub fn set(&mut self, namespace: &str, tag: &str, content: &str) -> std::io::Result<String> {
        self.store.create(namespace, tag, content)
    }

    /// Retrieve a memory's content by id.
    pub fn get(&self, namespace: &str, id: &str) -> std::io::Result<String> {
        self.store.read(namespace, id)
    }

    /// Delete a memory by id.
    pub fn delete(&mut self, namespace: &str, id: &str) -> std::io::Result<()> {
        self.store.delete(namespace, id)
    }

    /// List all memories in a namespace as `(id, content)` pairs.
    pub fn list(&self, namespace: &str) -> std::io::Result<Vec<(String, String)>> {
        self.store.list(namespace)
    }

    /// Find memories by exact tag match.
    pub fn find_by_tag(&self, namespace: &str, tag: &str) -> std::io::Result<Vec<(String, String)>> {
        let all = self.store.list(namespace)?;
        Ok(all
            .into_iter()
            .filter(|(id, _)| {
                // Load the entry to check its tag
                if let Ok(entry) = self.store.read_entry(namespace, id) {
                    entry.tag == tag
                } else {
                    false
                }
            })
            .collect())
    }
}
