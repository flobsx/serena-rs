//! Agent modes — current operational mode (code, chat, plan, etc.).

#[derive(Debug, Clone, Default)]
pub enum AgentMode {
    #[default]
    Code,
    Chat,
    Plan,
}
