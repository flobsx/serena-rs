//! Integration tests for serena-memory — MemoryManager and MemoryStore.

use serena_memory::manager::MemoryManager;
use serena_memory::storage::MemoryStore;
use std::fs;

#[test]
fn test_memory_store_create_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let id = store.create("project", "test-key", "Hello memory").unwrap();

    let content = store.read("project", &id).unwrap();
    assert_eq!(content, "Hello memory");
}

#[test]
fn test_memory_store_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    store.create("project", "key-a", "value a").unwrap();
    store.create("global", "key-b", "value b").unwrap();
    store.create("project", "key-c", "value c").unwrap();

    let project_memories = store.list("project").unwrap();
    let global_memories = store.list("global").unwrap();

    assert_eq!(project_memories.len(), 2);
    assert_eq!(global_memories.len(), 1);
}

#[test]
fn test_memory_store_update() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let id = store.create("project", "my-key", "original").unwrap();

    store.update("project", &id, "updated content").unwrap();
    let content = store.read("project", &id).unwrap();
    assert_eq!(content, "updated content");
}

#[test]
fn test_memory_store_delete() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let id = store.create("project", "delete-me", "to be deleted").unwrap();

    store.delete("project", &id).unwrap();
    let result = store.read("project", &id);
    assert!(result.is_err());
}

#[test]
fn test_memory_store_read_nonexistent() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let result = store.read("project", "nonexistent-id");
    assert!(result.is_err());
}

#[test]
fn test_memory_manager_crud() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let mut manager = MemoryManager::new(store);

    // Create a memory
    let id = manager.set("project", "my-tag", "My first memory").unwrap();

    // Read it back
    let content = manager.get("project", &id).unwrap();
    assert_eq!(content, "My first memory");

    // Find by tag
    let results = manager.find_by_tag("project", "my-tag").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, "My first memory");

    // Delete
    manager.delete("project", &id).unwrap();
    let result = manager.get("project", &id);
    assert!(result.is_err());
}

#[test]
fn test_memory_manager_global_and_project_separation() {
    let dir = tempfile::tempdir().unwrap();
    let store = MemoryStore::new(dir.path().join(".serena/memories"));
    let mut manager = MemoryManager::new(store);

    let pid = manager.set("project", "p-tag", "project memory").unwrap();
    let gid = manager.set("global", "g-tag", "global memory").unwrap();

    let project_all = manager.list("project").unwrap();
    let global_all = manager.list("global").unwrap();

    assert_eq!(project_all.len(), 1);
    assert_eq!(global_all.len(), 1);
    assert_ne!(pid, gid);
}
