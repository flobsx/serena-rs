//! Serena.rs — Configuration loading and validation
//!
//! Loads and validates Serena configuration from YAML files.
//! Supports project-level and global config with serde validation.

pub mod config;
pub mod project_config;
pub mod schema;

pub use config::SerenaConfig;
pub use project_config::ProjectConfig;
