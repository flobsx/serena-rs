//! Config/System tools — config queries and updates.
//!
//! Provides tools for getting and setting configuration values
//! stored in a simple JSON config file.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Global config path.
static CONFIG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Initialise the config storage path.
pub fn init_config(path: &str) {
    let mut guard = CONFIG_PATH.lock().unwrap();
    *guard = Some(PathBuf::from(path));
    tracing::info!(path = %path, "Config store initialised");
}

/// Get the config file path.
fn get_config_path() -> PathBuf {
    let guard = CONFIG_PATH.lock().unwrap();
    guard.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_default()
            .join(".serena")
            .join("config.json")
    })
}

/// Load config from file.
fn load_config() -> HashMap<String, serde_json::Value> {
    let path = get_config_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        HashMap::new()
    }
}

/// Save config to file.
fn save_config(config: &HashMap<String, serde_json::Value>) -> Result<(), String> {
    let path = get_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create config directory: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Cannot serialize config: {e}"))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Cannot write config: {e}"))?;
    Ok(())
}

/// Get a configuration value by key.
pub async fn get_config(key: &str) -> Result<serde_json::Value, String> {
    let config = load_config();
    config.get(key)
        .cloned()
        .ok_or_else(|| format!("Config key not found: '{key}'"))
}

/// Set a configuration value.
pub async fn set_config(key: &str, value: serde_json::Value) -> Result<(), String> {
    let mut config = load_config();
    config.insert(key.to_string(), value);
    save_config(&config)
}

/// List all configuration keys and values.
pub async fn list_config() -> Result<HashMap<String, serde_json::Value>, String> {
    Ok(load_config())
}

/// Register config tools (stub — tools are registered at the MCP level).
pub fn register() {
    tracing::info!("Config tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use serial_test::serial;

    fn init_test_config() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut guard = CONFIG_PATH.lock().unwrap();
        *guard = Some(path);
        dir
    }

    #[tokio::test]
    #[serial]
    async fn test_set_and_get_config() {
        let _d = init_test_config();
        set_config("theme", serde_json::json!("dark")).await.unwrap();

        let value = get_config("theme").await.unwrap();
        assert_eq!(value, "dark");
    }

    #[tokio::test]
    #[serial]
    async fn test_get_config_nonexistent() {
        let _d = init_test_config();
        let result = get_config("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[serial]
    async fn test_list_config() {
        let _d = init_test_config();
        set_config("a", serde_json::json!(1)).await.unwrap();
        set_config("b", serde_json::json!("two")).await.unwrap();

        let config = list_config().await.unwrap();
        assert_eq!(config.len(), 2);
        assert_eq!(config.get("a").unwrap(), &serde_json::json!(1));
        assert_eq!(config.get("b").unwrap(), &serde_json::json!("two"));
    }

    #[tokio::test]
    #[serial]
    async fn test_overwrite_config() {
        let _d = init_test_config();
        set_config("key", serde_json::json!("v1")).await.unwrap();
        set_config("key", serde_json::json!("v2")).await.unwrap();

        let value = get_config("key").await.unwrap();
        assert_eq!(value, "v2");
    }
}
