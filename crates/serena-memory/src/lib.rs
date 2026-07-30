//! Serena.rs — Persistent memory manager
//!
//! Stores and retrieves persistent memories (project and global).
//! Memories are written as markdown files under `.serena/memories/`.

pub mod manager;
pub mod storage;

pub use manager::MemoryManager;
pub use storage::MemoryStore;
