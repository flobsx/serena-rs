//! Serena.rs — Project management
//!
//! Manages project state: root detection, ignore specification,
//! file scanning, and language detection.

pub mod project;
pub mod scanner;
pub mod ignore;

pub use project::Project;
pub use scanner::FileScanner;
pub use ignore::IgnoreSpec;
