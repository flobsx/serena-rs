//! Shell tool — secure command execution.
//!
//! Provides `execute_shell_command` to run shell commands and
//! capture their stdout, stderr, and exit status.

use std::process::Command;

/// Execute a shell command and return its output.
pub async fn execute_shell_command(command: &str) -> Result<ShellCommandResult, String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", command])
            .output()
            .map_err(|e| format!("Failed to execute command: {e}"))?
    } else {
        Command::new("sh")
            .args(["-c", command])
            .output()
            .map_err(|e| format!("Failed to execute command: {e}"))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(ShellCommandResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        success: output.status.success(),
    })
}

/// Result of a shell command execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShellCommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

/// Register shell tool (stub — tools are registered at the MCP level).
pub fn register() {
    tracing::info!("Shell tools registered");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_shell_command_success() {
        let result = execute_shell_command("echo hello").await.unwrap();
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_shell_command_failure() {
        let result = execute_shell_command("exit 42").await.unwrap();
        assert!(!result.success);
        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    async fn test_execute_shell_command_stderr() {
        let result = execute_shell_command("echo error >&2").await.unwrap();
        assert!(result.success);
        assert!(result.stderr.contains("error"));
    }

    #[tokio::test]
    async fn test_execute_shell_command_empty() {
        let result = execute_shell_command("").await.unwrap();
        // Empty command succeeds (exits 0) or fails depending on platform
        // Either way, we should get a result without panicking
        assert!(result.stdout.is_empty() || result.exit_code != 0);
    }
}
