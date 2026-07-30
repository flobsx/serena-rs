//! Edit operations — types for symbol-level edits.

/// A symbol-level edit operation on source code.
///
/// Each variant targets a named symbol (function, struct, class, etc.)
/// identified by a regex pattern in the source text.
#[derive(Debug, Clone, PartialEq)]
pub enum EditOperation {
    /// Insert content before the matched symbol.
    InsertBefore {
        symbol: String,
        content: String,
    },
    /// Insert content after the matched symbol.
    InsertAfter {
        symbol: String,
        content: String,
    },
    /// Replace the body (content between braces/parens) of the matched symbol.
    ReplaceBody {
        symbol: String,
        content: String,
    },
    /// Delete the entire matched symbol (its declaration + body).
    Delete {
        symbol: String,
    },
    /// Rename the matched symbol to a new name.
    Rename {
        symbol: String,
        new_name: String,
    },
}
