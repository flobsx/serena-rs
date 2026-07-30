//! Symbolic code editor — insert, replace, delete at symbol level.
//!
//! The editor applies `EditOperation` values to source text using
//! regex-based symbol location, allowing LLM agents to surgically
//! modify code without parsing the full AST.

use regex::Regex;
use std::fmt;

use crate::operations::EditOperation;

/// Errors that can occur during editing.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorError {
    /// The target symbol was not found in the source.
    SymbolNotFound(String),
    /// The source text is empty.
    EmptySource,
    /// A regex error occurred when locating the symbol.
    RegexError(String),
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorError::SymbolNotFound(sym) => {
                write!(f, "symbol not found in source: {sym}")
            }
            EditorError::EmptySource => write!(f, "source text is empty"),
            EditorError::RegexError(msg) => write!(f, "regex error: {msg}"),
        }
    }
}

impl std::error::Error for EditorError {}

/// A symbolic code editor that applies `EditOperation` values to source text.
///
/// # Example
///
/// ```rust
/// use serena_editor::CodeEditor;
/// use serena_editor::operations::EditOperation;
///
/// let editor = CodeEditor::new();
/// let source = "fn hello() {\n    println!(\"hi\");\n}";
/// let op = EditOperation::InsertBefore {
///     symbol: "fn hello".to_string(),
///     content: "// greeting function\n".to_string(),
/// };
/// let result = editor.apply(source, &op).unwrap();
/// assert!(result.contains("// greeting function\nfn hello"));
/// ```
pub struct CodeEditor;

impl CodeEditor {
    /// Create a new `CodeEditor`.
    pub fn new() -> Self {
        Self
    }

    /// Apply an `EditOperation` to the given source text.
    ///
    /// Returns the modified source, or an `EditorError` if the symbol
    /// cannot be found or the source is empty.
    pub fn apply(&self, source: &str, operation: &EditOperation) -> Result<String, EditorError> {
        if source.is_empty() {
            return Err(EditorError::EmptySource);
        }

        match operation {
            EditOperation::InsertBefore { symbol, content } => {
                self.insert_before(source, symbol, content)
            }
            EditOperation::InsertAfter { symbol, content } => {
                self.insert_after(source, symbol, content)
            }
            EditOperation::ReplaceBody { symbol, content } => {
                self.replace_body(source, symbol, content)
            }
            EditOperation::Delete { symbol } => self.delete_symbol(source, symbol),
            EditOperation::Rename { symbol, new_name } => self.rename(source, symbol, new_name),
        }
    }

    /// Find the position (byte offset) of a symbol in the source using regex.
    fn find_symbol(&self, source: &str, symbol: &str) -> Result<usize, EditorError> {
        let pattern = Regex::new(&regex::escape(symbol))
            .map_err(|e| EditorError::RegexError(e.to_string()))?;

        pattern
            .find(source)
            .map(|m| m.start())
            .ok_or_else(|| EditorError::SymbolNotFound(symbol.to_string()))
    }

    /// Find the matching brace/paren/brace position starting from `open_pos`.
    /// Supports `{}`, `()`, `[]` pairs.
    fn find_matching_brace(&self, source: &str, open_pos: usize) -> Option<usize> {
        let bytes = source.as_bytes();
        let open_char = bytes.get(open_pos)?;
        let (open, close) = match open_char {
            b'{' => (b'{', b'}'),
            b'(' => (b'(', b')'),
            b'[' => (b'[', b']'),
            _ => return None,
        };

        let mut depth = 1;
        let mut pos = open_pos + 1;
        while pos < bytes.len() {
            match bytes[pos] {
                c if c == open => depth += 1,
                c if c == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(pos);
                    }
                }
                _ => {}
            }
            pos += 1;
        }
        None
    }

    fn insert_before(
        &self,
        source: &str,
        symbol: &str,
        content: &str,
    ) -> Result<String, EditorError> {
        let pos = self.find_symbol(source, symbol)?;
        let mut result = String::with_capacity(source.len() + content.len());
        result.push_str(&source[..pos]);
        result.push_str(content);
        result.push_str(&source[pos..]);
        Ok(result)
    }

    fn insert_after(
        &self,
        source: &str,
        symbol: &str,
        content: &str,
    ) -> Result<String, EditorError> {
        let pos = self.find_symbol(source, symbol)?;
        // Find end of the symbol's line (or the symbol itself)
        let end_of_symbol = self.find_end_of_symbol(source, pos)?;
        let mut result = String::with_capacity(source.len() + content.len());
        result.push_str(&source[..end_of_symbol]);
        result.push_str(content);
        result.push_str(&source[end_of_symbol..]);
        Ok(result)
    }

    fn replace_body(
        &self,
        source: &str,
        symbol: &str,
        content: &str,
    ) -> Result<String, EditorError> {
        let pos = self.find_symbol(source, symbol)?;

        // Find the opening brace after the symbol declaration
        let rest = &source[pos..];
        let brace_pos = rest.find('{').ok_or_else(|| {
            EditorError::RegexError(format!(
                "symbol '{symbol}' declaration has no opening brace"
            ))
        })?;
        let open_pos = pos + brace_pos;

        let close_pos = self
            .find_matching_brace(source, open_pos)
            .ok_or_else(|| {
                EditorError::RegexError(format!(
                    "symbol '{symbol}' has no matching closing brace"
                ))
            })?;

        let mut result = String::with_capacity(source.len());
        result.push_str(&source[..open_pos + 1]);
        result.push_str(content);
        if !content.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&source[close_pos..]);
        Ok(result)
    }

    fn delete_symbol(
        &self,
        source: &str,
        symbol: &str,
    ) -> Result<String, EditorError> {
        let pos = self.find_symbol(source, symbol)?;

        // Find the end of the symbol (its closing brace)
        let rest = &source[pos..];
        let brace_pos = rest.find('{').ok_or_else(|| {
            EditorError::RegexError(format!(
                "symbol '{symbol}' declaration has no opening brace"
            ))
        })?;
        let open_pos = pos + brace_pos;

        let close_pos = self
            .find_matching_brace(source, open_pos)
            .ok_or_else(|| {
                EditorError::RegexError(format!(
                    "symbol '{symbol}' has no matching closing brace"
                ))
            })?;

        // Also remove trailing blank line after the symbol
        let after = &source[close_pos + 1..];
        let skip = if after.starts_with("\n\n") {
            2
        } else if after.starts_with('\n') {
            1
        } else {
            0
        };

        let mut result = String::with_capacity(source.len());
        result.push_str(&source[..pos]);
        result.push_str(&source[close_pos + 1 + skip..]);
        Ok(result)
    }

    fn rename(
        &self,
        source: &str,
        symbol: &str,
        new_name: &str,
    ) -> Result<String, EditorError> {
        let pos = self.find_symbol(source, symbol)?;

        let mut result = String::with_capacity(source.len());
        result.push_str(&source[..pos]);
        result.push_str(new_name);
        result.push_str(&source[pos + symbol.len()..]);
        Ok(result)
    }

    /// Find the end of a symbol declaration — the end of the line containing
    /// the symbol match.
    fn find_end_of_symbol(&self, source: &str, match_start: usize) -> Result<usize, EditorError> {
        let rest = &source[match_start..];
        match rest.find('\n') {
            Some(nl_pos) => Ok(match_start + nl_pos),
            None => Ok(source.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_not_found_error() {
        let editor = CodeEditor::new();
        let err = editor.apply("fn foo() {}", &EditOperation::Delete {
            symbol: "nonexistent".to_string(),
        }).unwrap_err();
        assert!(matches!(err, EditorError::SymbolNotFound(_)));
    }

    #[test]
    fn test_empty_source_error() {
        let editor = CodeEditor::new();
        let err = editor.apply("", &EditOperation::InsertBefore {
            symbol: "x".to_string(),
            content: "y".to_string(),
        }).unwrap_err();
        assert_eq!(err, EditorError::EmptySource);
    }
}
