//! Integration tests for serena-editor — CodeEditor and EditOperation.

use serena_editor::operations::EditOperation;
use serena_editor::CodeEditor;

#[test]
fn test_insert_before_symbol() {
    let source = "fn foo() {\n    println!(\"hello\");\n}\n\nfn bar() {\n    println!(\"world\");\n}";
    let editor = CodeEditor::new();
    let op = EditOperation::InsertBefore {
        symbol: "fn foo".to_string(),
        content: "// This is foo\n".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    assert!(result.contains("// This is foo\nfn foo"));
    assert!(result.contains("fn bar"));
}

#[test]
fn test_insert_after_symbol() {
    let source = "fn foo() {\n    return 1;\n}\n\nfn bar() {\n    return 2;\n}";
    let editor = CodeEditor::new();
    let op = EditOperation::InsertAfter {
        symbol: "fn foo".to_string(),
        content: "\n// end of foo\n".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    eprintln!("--- insert_after result ---\n{result}\n---");
    assert!(result.contains("// end of foo"));
    assert!(result.contains("fn bar"));
}

#[test]
fn test_replace_body_of_function() {
    let source = "fn foo() {\n    let x = 1;\n    x + 1\n}";
    let editor = CodeEditor::new();
    let op = EditOperation::ReplaceBody {
        symbol: "fn foo".to_string(),
        content: "    42".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    eprintln!("--- replace_body result ---\n{result}\n---");
    assert!(result.contains("    42"));
}

#[test]
fn test_delete_symbol() {
    let source = "fn keep() {}\n\nfn remove_me() {\n    println!(\"bye\");\n}\n\nfn also_keep() {}";
    let editor = CodeEditor::new();
    let op = EditOperation::Delete {
        symbol: "fn remove_me".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    assert!(result.contains("fn keep()"));
    assert!(result.contains("fn also_keep"));
    assert!(!result.contains("remove_me"));
}

#[test]
fn test_rename_symbol() {
    let source = "fn old_name() {\n    println!(\"hello\");\n}";
    let editor = CodeEditor::new();
    let op = EditOperation::Rename {
        symbol: "fn old_name".to_string(),
        new_name: "fn new_name".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    assert!(!result.contains("fn old_name"));
    assert!(result.contains("fn new_name"));
}

#[test]
fn test_insert_before_class_in_java() {
    let source = "public class Foo {\n    int x;\n}\n\npublic class Bar {\n    int y;\n}";
    let editor = CodeEditor::new();
    let op = EditOperation::InsertBefore {
        symbol: "public class Bar".to_string(),
        content: "// New class below\n".to_string(),
    };
    let result = editor.apply(source, &op).unwrap();
    assert!(result.contains("// New class below\npublic class Bar"));
}

#[test]
fn test_edit_on_empty_source_returns_error() {
    let editor = CodeEditor::new();
    let op = EditOperation::InsertBefore {
        symbol: "fn main".to_string(),
        content: "// nothing\n".to_string(),
    };
    let result = editor.apply("", &op);
    assert!(result.is_err());
}

#[test]
fn test_edit_symbol_not_found_returns_error() {
    let editor = CodeEditor::new();
    let op = EditOperation::Delete {
        symbol: "nonexistent_symbol".to_string(),
    };
    let result = editor.apply("fn foo() {}", &op);
    assert!(result.is_err());
}
