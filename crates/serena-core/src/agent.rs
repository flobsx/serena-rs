//! Core agent orchestration — ties config, project, and tools together.
//!
//! The SerenaAgent is the central orchestrator: it loads config,
//! activates projects, manages modes, and routes tool calls.

use serena_config::SerenaConfig;
use serena_project::Project;
use serena_memory::{MemoryManager, MemoryStore};

pub struct SerenaAgent {
    pub config: SerenaConfig,
    pub project: Option<Project>,
    pub memory: MemoryManager,
}

impl SerenaAgent {
    pub fn new(config: SerenaConfig) -> Self {
        let store = MemoryStore::new(".serena/memories");
        Self {
            config,
            project: None,
            memory: MemoryManager::new(store),
        }
    }

    pub fn activate_project(&mut self, _path: &str) {
        // TODO: load and activate project
    }
}
