//! Integration tests for serena-project — Project, FileScanner, IgnoreSpec.

use serena_project::ignore::IgnoreSpec;
use serena_project::scanner::FileScanner;
use serena_project::Project;
use std::fs;

fn setup_test_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create some test files
    fs::write(root.join("main.rs"), "fn main() {}").unwrap();
    fs::write(root.join("lib.rs"), "pub fn hello() {}").unwrap();
    fs::write(root.join("README.md"), "# Test").unwrap();
    fs::write(root.join(".gitignore"), "*.md\ntarget/\n").unwrap();

    // Create a nested dir
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("app.rs"), "fn app() {}").unwrap();
    fs::write(root.join("src").join("data.rs"), "fn data() {}").unwrap();

    // Create an ignored dir
    fs::create_dir_all(root.join("target")).unwrap();
    fs::write(root.join("target").join("build.o"), "binary").unwrap();

    dir
}

#[test]
fn test_project_open_detects_root() {
    let dir = setup_test_dir();
    let project = Project::open(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(project.root(), dir.path().canonicalize().unwrap());
}

#[test]
fn test_project_open_invalid_path() {
    let result = Project::open("/nonexistent/path/that/does/not/exist");
    assert!(result.is_err());
}

#[test]
fn test_project_open_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    // An empty dir has no .serena config but should still open
    let project = Project::open(dir.path().to_str().unwrap()).unwrap();
    assert!(project.root().exists());
}

#[test]
fn test_ignore_spec_from_gitignore() {
    let dir = setup_test_dir();
    let spec = IgnoreSpec::from_gitignore(dir.path().join(".gitignore")).unwrap();
    assert!(spec.is_ignored("target/build.o"));
    assert!(spec.is_ignored("README.md"));
    assert!(!spec.is_ignored("main.rs"));
    assert!(!spec.is_ignored("src/app.rs"));
}

#[test]
fn test_ignore_spec_empty() {
    let spec = IgnoreSpec::empty();
    assert!(!spec.is_ignored("anything.txt"));
    assert!(!spec.is_ignored("target/build.o"));
}

#[test]
fn test_file_scanner_finds_rust_files() {
    let dir = setup_test_dir();
    let spec = IgnoreSpec::from_gitignore(dir.path().join(".gitignore")).unwrap();
    let scanner = FileScanner::new(spec);
    let files = scanner.scan(dir.path(), &["rs"]).unwrap();

    let names: Vec<String> = files.iter()
        .map(|p| p.strip_prefix(dir.path()).unwrap().to_string_lossy().to_string())
        .collect();

    assert!(names.contains(&"main.rs".to_string()));
    assert!(names.contains(&"lib.rs".to_string()));
    assert!(names.contains(&"src/app.rs".to_string()));
    assert!(names.contains(&"src/data.rs".to_string()));
    // target/ files should be ignored
    assert!(!names.iter().any(|n| n.starts_with("target")));
    // Non-matching extension should not be included
    assert!(!names.iter().any(|n| n.ends_with(".md")));
}

#[test]
fn test_file_scanner_ignored_dir_excluded() {
    let dir = setup_test_dir();
    let spec = IgnoreSpec::from_gitignore(dir.path().join(".gitignore")).unwrap();
    let scanner = FileScanner::new(spec);
    let files = scanner.scan(dir.path(), &["o"]).unwrap();

    let names: Vec<String> = files.iter()
        .map(|p| p.strip_prefix(dir.path()).unwrap().to_string_lossy().to_string())
        .collect();

    // target/ is in .gitignore, so build.o should not appear
    assert!(!names.contains(&"target/build.o".to_string()));
}
