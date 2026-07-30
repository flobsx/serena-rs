//! Agent execution context — current project, mode, settings.

use serena_project::Project;
use crate::mode::AgentMode;

pub struct AgentContext {
    pub project: Option<Project>,
    pub mode: AgentMode,
}

impl AgentContext {
    pub fn new() -> Self {
        Self {
            project: None,
            mode: AgentMode::default(),
        }
    }
}
