//! Serena.rs — Shared utilities
//!
//! Common utility functions shared across all crates:
//! text processing, path handling, filesystem operations, token counting.

pub mod text;
pub mod path;
pub mod fs;
pub mod tokens;

pub use text::*;
pub use path::*;
pub use fs::*;
pub use tokens::*;
