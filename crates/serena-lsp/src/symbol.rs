//! Symbol types and retrieval.
//!
//! Defines the core symbol representation used throughout Serena.rs
//! for code navigation, indexing, and semantic analysis.
//! Conversions from `lsp_types` types enable integration with
//! language servers.

use serde::{Deserialize, Serialize};
use lsp_types as lspt;

/// A symbol in a codebase — function, class, variable, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Symbol {
    /// The symbol name (e.g. `"parse_config"`)
    pub name: String,
    /// The kind of symbol
    pub kind: SymbolKind,
    /// Optional detail (e.g. type signature `"fn(&str) -> Result<Config>"`)
    pub detail: Option<String>,
    /// The full range of the symbol (declaration or definition)
    pub range: Range,
    /// The range of the identifier itself (smaller than `range`)
    pub selection_range: Option<Range>,
    /// Child symbols (e.g. methods on a struct)
    pub children: Vec<Symbol>,
    /// Optional tags (e.g. deprecated, unused)
    pub tags: Vec<SymbolTag>,
    /// Optional deprecation message
    pub deprecated: Option<String>,
}

/// Kinds of symbols.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Package,
    Class,
    Method,
    Property,
    Field,
    Constructor,
    Enum,
    Interface,
    Function,
    Variable,
    Constant,
    String,
    Number,
    Boolean,
    Array,
    Object,
    Key,
    Null,
    EnumMember,
    Struct,
    Event,
    Operator,
    TypeParameter,
    Unknown(i32),
}

/// Tags that can be attached to a symbol.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SymbolTag {
    Deprecated,
    Unused,
    Unknown(i32),
}

/// A position in a text document (0-indexed).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: u32,
    /// Column number (0-indexed, UTF-16 code units)
    pub column: u32,
}

/// A range in a text document.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Range {
    /// Start position (inclusive)
    pub start: Position,
    /// End position (exclusive)
    pub end: Position,
}

/// A location in a text document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Location {
    /// The document URI
    pub uri: String,
    /// The range within the document
    pub range: Range,
}

/// Hover information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HoverInfo {
    /// The hover contents (markdown or plain text)
    pub contents: Vec<MarkupContent>,
    /// The range of the symbol being hovered over
    pub range: Option<Range>,
}

/// Markup content for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarkupContent {
    /// The kind of markup (markdown or plaintext)
    pub kind: MarkupKind,
    /// The content value
    pub value: String,
}

/// Kinds of markup.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MarkupKind {
    PlainText,
    Markdown,
}

/// A code action / quick-fix.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeAction {
    /// Title shown in the UI
    pub title: String,
    /// Optional kind (e.g. "quickfix", "refactor")
    pub kind: Option<String>,
    /// Optional diagnostics this action addresses
    pub diagnostics: Vec<Diagnostic>,
    /// The edit to apply, if known
    pub edit: Option<WorkspaceEdit>,
    /// Whether this is a preferred action
    pub is_preferred: Option<bool>,
}

/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    /// The range where the diagnostic applies
    pub range: Range,
    /// Severity (1=error, 2=warning, 3=info, 4=hint)
    pub severity: Option<i32>,
    /// The diagnostic message
    pub message: String,
    /// Optional source (e.g. "rustc", "eslint")
    pub source: Option<String>,
    /// Optional diagnostic code
    pub code: Option<String>,
    /// Related information
    pub related_information: Vec<DiagnosticRelatedInfo>,
}

/// Related information for a diagnostic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiagnosticRelatedInfo {
    pub location: Location,
    pub message: String,
}

/// A workspace edit (set of text changes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkspaceEdit {
    /// Changes per document URI
    pub changes: Vec<TextEdit>,
}

/// A single text edit to apply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextEdit {
    /// The range to replace
    pub range: Range,
    /// The new text
    pub new_text: String,
}

/// A completion item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: Option<CompletionItemKind>,
    pub detail: Option<String>,
    pub documentation: Option<String>,
}

/// Kinds of completion items.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CompletionItemKind {
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
    Unknown(i32),
}

// ============================================================================
// Conversions from lsp_types
// ============================================================================

/// Convert an lsp_types SymbolKind to our SymbolKind by comparing via PartialEq.
/// (lsp-types 0.95 uses newtype(i32) with private inner field.)
fn lspt_symbol_kind_to_ours(kind: &lspt::SymbolKind) -> SymbolKind {
    use lspt::SymbolKind as L;
    if kind == &L::FILE { SymbolKind::File }
    else if kind == &L::MODULE { SymbolKind::Module }
    else if kind == &L::NAMESPACE { SymbolKind::Namespace }
    else if kind == &L::PACKAGE { SymbolKind::Package }
    else if kind == &L::CLASS { SymbolKind::Class }
    else if kind == &L::METHOD { SymbolKind::Method }
    else if kind == &L::PROPERTY { SymbolKind::Property }
    else if kind == &L::FIELD { SymbolKind::Field }
    else if kind == &L::CONSTRUCTOR { SymbolKind::Constructor }
    else if kind == &L::ENUM { SymbolKind::Enum }
    else if kind == &L::INTERFACE { SymbolKind::Interface }
    else if kind == &L::FUNCTION { SymbolKind::Function }
    else if kind == &L::VARIABLE { SymbolKind::Variable }
    else if kind == &L::CONSTANT { SymbolKind::Constant }
    else if kind == &L::STRING { SymbolKind::String }
    else if kind == &L::NUMBER { SymbolKind::Number }
    else if kind == &L::BOOLEAN { SymbolKind::Boolean }
    else if kind == &L::ARRAY { SymbolKind::Array }
    else if kind == &L::OBJECT { SymbolKind::Object }
    else if kind == &L::KEY { SymbolKind::Key }
    else if kind == &L::NULL { SymbolKind::Null }
    else if kind == &L::ENUM_MEMBER { SymbolKind::EnumMember }
    else if kind == &L::STRUCT { SymbolKind::Struct }
    else if kind == &L::EVENT { SymbolKind::Event }
    else if kind == &L::OPERATOR { SymbolKind::Operator }
    else if kind == &L::TYPE_PARAMETER { SymbolKind::TypeParameter }
    else { SymbolKind::Unknown(0) }
}

fn lspt_symbol_tag_to_ours(tag: lspt::SymbolTag) -> SymbolTag {
    if tag == lspt::SymbolTag::DEPRECATED { SymbolTag::Deprecated }
    else { SymbolTag::Unknown(0) }
}

fn lspt_completion_item_kind_to_ours(kind: &lspt::CompletionItemKind) -> CompletionItemKind {
    use lspt::CompletionItemKind as L;
    if kind == &L::TEXT { CompletionItemKind::Text }
    else if kind == &L::METHOD { CompletionItemKind::Method }
    else if kind == &L::FUNCTION { CompletionItemKind::Function }
    else if kind == &L::CONSTRUCTOR { CompletionItemKind::Constructor }
    else if kind == &L::FIELD { CompletionItemKind::Field }
    else if kind == &L::VARIABLE { CompletionItemKind::Variable }
    else if kind == &L::CLASS { CompletionItemKind::Class }
    else if kind == &L::INTERFACE { CompletionItemKind::Interface }
    else if kind == &L::MODULE { CompletionItemKind::Module }
    else if kind == &L::PROPERTY { CompletionItemKind::Property }
    else if kind == &L::UNIT { CompletionItemKind::Unit }
    else if kind == &L::VALUE { CompletionItemKind::Value }
    else if kind == &L::ENUM { CompletionItemKind::Enum }
    else if kind == &L::KEYWORD { CompletionItemKind::Keyword }
    else if kind == &L::SNIPPET { CompletionItemKind::Snippet }
    else if kind == &L::COLOR { CompletionItemKind::Color }
    else if kind == &L::FILE { CompletionItemKind::File }
    else if kind == &L::REFERENCE { CompletionItemKind::Reference }
    else if kind == &L::FOLDER { CompletionItemKind::Folder }
    else if kind == &L::ENUM_MEMBER { CompletionItemKind::EnumMember }
    else if kind == &L::CONSTANT { CompletionItemKind::Constant }
    else if kind == &L::STRUCT { CompletionItemKind::Struct }
    else if kind == &L::EVENT { CompletionItemKind::Event }
    else if kind == &L::OPERATOR { CompletionItemKind::Operator }
    else if kind == &L::TYPE_PARAMETER { CompletionItemKind::TypeParameter }
    else { CompletionItemKind::Unknown(0) }
}

impl From<lspt::SymbolKind> for SymbolKind {
    fn from(kind: lspt::SymbolKind) -> Self {
        lspt_symbol_kind_to_ours(&kind)
    }
}

impl From<&lspt::SymbolKind> for SymbolKind {
    fn from(kind: &lspt::SymbolKind) -> Self {
        lspt_symbol_kind_to_ours(kind)
    }
}

impl From<SymbolKind> for lspt::SymbolKind {
    fn from(kind: SymbolKind) -> Self {
        use lspt::SymbolKind as L;
        match kind {
            SymbolKind::File => L::FILE,
            SymbolKind::Module => L::MODULE,
            SymbolKind::Namespace => L::NAMESPACE,
            SymbolKind::Package => L::PACKAGE,
            SymbolKind::Class => L::CLASS,
            SymbolKind::Method => L::METHOD,
            SymbolKind::Property => L::PROPERTY,
            SymbolKind::Field => L::FIELD,
            SymbolKind::Constructor => L::CONSTRUCTOR,
            SymbolKind::Enum => L::ENUM,
            SymbolKind::Interface => L::INTERFACE,
            SymbolKind::Function => L::FUNCTION,
            SymbolKind::Variable => L::VARIABLE,
            SymbolKind::Constant => L::CONSTANT,
            SymbolKind::String => L::STRING,
            SymbolKind::Number => L::NUMBER,
            SymbolKind::Boolean => L::BOOLEAN,
            SymbolKind::Array => L::ARRAY,
            SymbolKind::Object => L::OBJECT,
            SymbolKind::Key => L::KEY,
            SymbolKind::Null => L::NULL,
            SymbolKind::EnumMember => L::ENUM_MEMBER,
            SymbolKind::Struct => L::STRUCT,
            SymbolKind::Event => L::EVENT,
            SymbolKind::Operator => L::OPERATOR,
            SymbolKind::TypeParameter => L::TYPE_PARAMETER,
            SymbolKind::Unknown(_) => L::FILE,
        }
    }
}

impl From<lspt::SymbolTag> for SymbolTag {
    fn from(tag: lspt::SymbolTag) -> Self {
        lspt_symbol_tag_to_ours(tag)
    }
}

impl From<&lspt::SymbolTag> for SymbolTag {
    fn from(tag: &lspt::SymbolTag) -> Self {
        lspt_symbol_tag_to_ours(tag.clone())
    }
}

impl From<lspt::Position> for Position {
    fn from(pos: lspt::Position) -> Self {
        Self {
            line: pos.line,
            column: pos.character,
        }
    }
}

impl From<Position> for lspt::Position {
    fn from(pos: Position) -> Self {
        Self {
            line: pos.line,
            character: pos.column,
        }
    }
}

impl From<lspt::Range> for Range {
    fn from(r: lspt::Range) -> Self {
        Self {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl From<Range> for lspt::Range {
    fn from(r: Range) -> Self {
        Self {
            start: r.start.into(),
            end: r.end.into(),
        }
    }
}

impl From<lspt::Location> for Location {
    fn from(loc: lspt::Location) -> Self {
        Self {
            uri: loc.uri.to_string(),
            range: loc.range.into(),
        }
    }
}

impl From<lspt::MarkupContent> for MarkupContent {
    fn from(mc: lspt::MarkupContent) -> Self {
        Self {
            kind: mc.kind.into(),
            value: mc.value,
        }
    }
}

impl From<lspt::MarkupKind> for MarkupKind {
    fn from(kind: lspt::MarkupKind) -> Self {
        match kind {
            lspt::MarkupKind::PlainText => Self::PlainText,
            lspt::MarkupKind::Markdown => Self::Markdown,
        }
    }
}

impl From<lspt::CompletionItemKind> for CompletionItemKind {
    fn from(kind: lspt::CompletionItemKind) -> Self {
        lspt_completion_item_kind_to_ours(&kind)
    }
}

impl From<&lspt::CompletionItemKind> for CompletionItemKind {
    fn from(kind: &lspt::CompletionItemKind) -> Self {
        lspt_completion_item_kind_to_ours(kind)
    }
}

// ============================================================================
// LspSymbol — conversion from lsp_types::DocumentSymbol
// ============================================================================

/// A high-level symbol that can be built from an LSP `DocumentSymbol`.
impl Symbol {
    /// Create a `Symbol` from an LSP `DocumentSymbol`.
    pub fn from_lsp(sym: &lspt::DocumentSymbol) -> Self {
        Self {
            name: sym.name.clone(),
            kind: SymbolKind::from(&sym.kind),
            detail: sym.detail.clone(),
            range: sym.range.into(),
            selection_range: Some(sym.selection_range.into()),
            children: sym.children.as_ref()
                .map(|kids| kids.iter().map(Self::from_lsp).collect())
                .unwrap_or_default(),
            tags: sym.tags.as_ref()
                .map(|t| t.iter().map(|t| SymbolTag::from(t)).collect())
                .unwrap_or_default(),
            deprecated: if sym.tags.as_ref().map_or(false, |t| t.contains(&lspt::SymbolTag::DEPRECATED)) {
                Some("deprecated".to_string())
            } else {
                None
            },
        }
    }

    /// Create a `Symbol` from an LSP `SymbolInformation`.
    pub fn from_symbol_info(info: &lspt::SymbolInformation) -> Self {
        Self {
            name: info.name.clone(),
            kind: SymbolKind::from(&info.kind),
            detail: None,
            range: info.location.range.into(),
            selection_range: None,
            children: Vec::new(),
            tags: info.tags.as_ref()
                .map(|t| t.iter().map(|t| SymbolTag::from(t)).collect())
                .unwrap_or_default(),
            deprecated: if info.deprecated.unwrap_or(false) {
                Some("deprecated".to_string())
            } else {
                None
            },
        }
    }

    /// Flatten this symbol and its children into a list.
    pub fn flatten(&self) -> Vec<&Symbol> {
        let mut result = vec![self];
        for child in &self.children {
            result.extend(child.flatten());
        }
        result
    }
}

// ============================================================================
// Helpers
// ============================================================================

impl Position {
    /// Create a new position.
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

impl Range {
    /// Create a new range.
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_conversion() {
        let pos = Position::new(5, 10);
        let lsp_pos: lspt::Position = pos.into();
        assert_eq!(lsp_pos.line, 5);
        assert_eq!(lsp_pos.character, 10);
        let back: Position = lsp_pos.into();
        assert_eq!(back.line, 5);
        assert_eq!(back.column, 10);
    }

    #[test]
    fn test_range_conversion() {
        let range = Range::new(Position::new(1, 2), Position::new(3, 4));
        let lsp_range: lspt::Range = range.into();
        assert_eq!(lsp_range.start.line, 1);
        assert_eq!(lsp_range.start.character, 2);
        assert_eq!(lsp_range.end.line, 3);
        assert_eq!(lsp_range.end.character, 4);
    }

    #[test]
    fn test_symbol_kind_conversion() {
        let pairs: &[(lspt::SymbolKind, SymbolKind)] = &[
            (lspt::SymbolKind::FILE, SymbolKind::File),
            (lspt::SymbolKind::FUNCTION, SymbolKind::Function),
            (lspt::SymbolKind::CLASS, SymbolKind::Class),
            (lspt::SymbolKind::MODULE, SymbolKind::Module),
            (lspt::SymbolKind::STRUCT, SymbolKind::Struct),
            (lspt::SymbolKind::ENUM, SymbolKind::Enum),
            (lspt::SymbolKind::INTERFACE, SymbolKind::Interface),
            (lspt::SymbolKind::METHOD, SymbolKind::Method),
            (lspt::SymbolKind::VARIABLE, SymbolKind::Variable),
            (lspt::SymbolKind::CONSTANT, SymbolKind::Constant),
            (lspt::SymbolKind::PROPERTY, SymbolKind::Property),
            (lspt::SymbolKind::FIELD, SymbolKind::Field),
            (lspt::SymbolKind::NAMESPACE, SymbolKind::Namespace),
            (lspt::SymbolKind::PACKAGE, SymbolKind::Package),
            (lspt::SymbolKind::ENUM_MEMBER, SymbolKind::EnumMember),
            (lspt::SymbolKind::TYPE_PARAMETER, SymbolKind::TypeParameter),
        ];
        for (lsp, ours) in pairs {
            let converted: SymbolKind = lsp.clone().into();
            assert_eq!(converted, *ours, "SymbolKind mismatch for {lsp:?}");
            // Round-trip
            let back: lspt::SymbolKind = converted.into();
            let round: SymbolKind = back.into();
            assert_eq!(round, *ours, "Round-trip failed for {lsp:?}");
        }
    }

    #[test]
    fn test_from_lsp_document_symbol() {
        let lsp_sym = lspt::DocumentSymbol {
            name: "foo".to_string(),
            detail: Some("fn() -> i32".to_string()),
            kind: lspt::SymbolKind::FUNCTION,
            tags: Some(vec![lspt::SymbolTag::DEPRECATED]),
            deprecated: Some(true),
            range: lspt::Range {
                start: lspt::Position { line: 0, character: 0 },
                end: lspt::Position { line: 10, character: 0 },
            },
            selection_range: lspt::Range {
                start: lspt::Position { line: 0, character: 4 },
                end: lspt::Position { line: 0, character: 7 },
            },
            children: Some(vec![lspt::DocumentSymbol {
                name: "bar".to_string(),
                detail: None,
                kind: lspt::SymbolKind::VARIABLE,
                tags: None,
                deprecated: None,
                range: lspt::Range {
                    start: lspt::Position { line: 1, character: 4 },
                    end: lspt::Position { line: 1, character: 10 },
                },
                selection_range: lspt::Range {
                    start: lspt::Position { line: 1, character: 4 },
                    end: lspt::Position { line: 1, character: 7 },
                },
                children: None,
            }]),
        };

        let sym = Symbol::from_lsp(&lsp_sym);
        assert_eq!(sym.name, "foo");
        assert_eq!(sym.kind, SymbolKind::Function);
        assert_eq!(sym.detail, Some("fn() -> i32".to_string()));
        assert!(sym.tags.contains(&SymbolTag::Deprecated));
        assert_eq!(sym.children.len(), 1);
        assert_eq!(sym.children[0].name, "bar");
        assert_eq!(sym.children[0].kind, SymbolKind::Variable);
    }

    #[test]
    fn test_flatten() {
        let child = Symbol {
            name: "child".into(),
            kind: SymbolKind::Variable,
            detail: None,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            selection_range: None,
            children: vec![],
            tags: vec![],
            deprecated: None,
        };
        let parent = Symbol {
            name: "parent".into(),
            kind: SymbolKind::Function,
            detail: None,
            range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            selection_range: None,
            children: vec![child],
            tags: vec![],
            deprecated: None,
        };
        let flat = parent.flatten();
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].name, "parent");
        assert_eq!(flat[1].name, "child");
    }
}
