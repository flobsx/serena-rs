//! Serena.rs — Symbolic code editing
//!
//! Edit code at the symbol level: insert before/after symbols,
//! replace symbol bodies, delete symbols, rename symbols.
//! Operates on file text with regex-based symbol boundaries.

pub mod editor;
pub mod operations;

pub use editor::{CodeEditor, EditorError};
pub use operations::EditOperation;
