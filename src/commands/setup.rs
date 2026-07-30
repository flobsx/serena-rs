//! Setup command — configuration assistant.
//!
//! Detects project languages and configures LSP settings
//! by writing to `.serena/config.yaml`.

use std::collections::BTreeMap;
use std::path::Path;

/// Supported programming languages and their default LSPs.
pub fn default_lsps() -> BTreeMap<&'static str, Vec<&'static str>> {
    BTreeMap::from([
        ("rust", vec!["rust-analyzer"]),
        ("python", vec!["pyright", "ruff"]),
        ("typescript", vec!["typescript-language-server", "eslint"]),
        ("javascript", vec!["typescript-language-server", "eslint"]),
        ("go", vec!["gopls"]),
        ("java", vec!["jdtls"]),
        ("csharp", vec!["omnisharp"]),
        ("cpp", vec!["clangd"]),
        ("ruby", vec!["solargraph"]),
        ("php", vec!["intelephense"]),
    ])
}

/// Detect languages present in a project directory.
///
/// Scans for well-known files/extensions and returns a sorted list
/// of detected language names.
pub fn detect_languages(project_path: &str) -> Vec<String> {
    let root = Path::new(project_path);
    if !root.is_dir() {
        return Vec::new();
    }

    let mut detected = Vec::new();

    // Build a map of indicator files → language name
    let indicators: Vec<(&str, &str)> = vec![
        ("Cargo.toml", "rust"),
        ("pyproject.toml", "python"),
        ("setup.py", "python"),
        ("requirements.txt", "python"),
        ("Pipfile", "python"),
        ("package.json", "typescript"),
        ("tsconfig.json", "typescript"),
        ("go.mod", "go"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("*.csproj", "csharp"),
        ("CMakeLists.txt", "cpp"),
        ("Gemfile", "ruby"),
        ("composer.json", "php"),
    ];

    for (filename, lang) in &indicators {
        let filepath = root.join(filename);
        if filepath.exists() {
            if !detected.contains(&lang.to_string()) {
                detected.push(lang.to_string());
            }
        }
    }

    detected.sort();
    detected
}

/// Detect LSPs for the given languages.
pub fn detect_lsps(languages: &[String]) -> BTreeMap<String, Vec<String>> {
    let default_lsps_map = default_lsps();
    let mut result = BTreeMap::new();

    for lang in languages {
        let lsps = default_lsps_map
            .get(lang.as_str())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        result.insert(lang.clone(), lsps);
    }

    result
}

#[derive(Debug, PartialEq)]
pub enum SetupError {
    NotAProject(String),
    IoError(String),
}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetupError::NotAProject(p) => write!(f, "not a valid project directory: {p}"),
            SetupError::IoError(msg) => write!(f, "IO error: {msg}"),
        }
    }
}

impl std::error::Error for SetupError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_default_lsps_contains_rust() {
        let lsps = default_lsps();
        assert!(lsps.contains_key("rust"), "rust should have default LSPs");
        assert!(
            lsps.get("rust").unwrap().contains(&"rust-analyzer"),
            "rust-analyzer should be default for rust"
        );
    }

    #[test]
    fn test_detect_languages_rust_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        // Create Cargo.toml to simulate a Rust project
        fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"\n")
            .unwrap();

        let languages = detect_languages(path);
        assert!(languages.contains(&"rust".to_string()), "should detect rust");
    }

    #[test]
    fn test_detect_languages_python_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        fs::write(dir.path().join("setup.py"), "from setuptools import setup\n").unwrap();

        let languages = detect_languages(path);
        assert!(languages.contains(&"python".to_string()), "should detect python");
    }

    #[test]
    fn test_detect_languages_typescript_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        fs::write(dir.path().join("package.json"), "{}\n").unwrap();

        let languages = detect_languages(path);
        assert!(languages.contains(&"typescript".to_string()), "should detect typescript");
    }

    #[test]
    fn test_detect_languages_multi_lang_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        fs::write(dir.path().join("setup.py"), "").unwrap();

        let languages = detect_languages(path);
        assert_eq!(languages.len(), 2, "should detect 2 languages");
        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"rust".to_string()));
    }

    #[test]
    fn test_detect_languages_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_str().unwrap();

        let languages = detect_languages(path);
        assert!(languages.is_empty(), "no languages in empty dir");
    }

    #[test]
    fn test_detect_languages_nonexistent_path() {
        let languages = detect_languages("/nonexistent/path");
        assert!(languages.is_empty(), "no languages in nonexistent path");
    }

    #[test]
    fn test_detect_lsps_for_detected_languages() {
        let languages = vec!["rust".to_string(), "python".to_string()];
        let lsps = detect_lsps(&languages);

        assert!(lsps.contains_key("rust"));
        assert!(lsps.contains_key("python"));

        let rust_lsps = lsps.get("rust").unwrap();
        assert!(rust_lsps.contains(&"rust-analyzer".to_string()));

        let python_lsps = lsps.get("python").unwrap();
        assert!(python_lsps.contains(&"pyright".to_string()));
    }

    #[test]
    fn test_detect_lsps_unknown_language() {
        let languages = vec!["unknown_lang".to_string()];
        let lsps = detect_lsps(&languages);

        assert!(lsps.contains_key("unknown_lang"));
        assert!(
            lsps.get("unknown_lang").unwrap().is_empty(),
            "unknown language has no LSPs"
        );
    }
}
