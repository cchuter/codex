//! Default configuration for new installations

use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use tokio::fs;

const DEFAULT_CONFIG_TOML: &str = r#"# OsmiFlow Configuration

[model_providers.osmi]
name = "OSMI"
api_base_url = "https://models.osmi.ai/v1"
default_model_id = "claude-3-5-sonnet-20241022"

[proxy.osmi]
provider_id = "osmi"
model_id = "claude-3-5-sonnet-20241022"
max_concurrent = 3
max_queued = 20
timeout_ms = 180000
"#;

/// Ensures a default config exists at ~/.osmiflow/config.toml if it doesn't already exist.
/// Skips creation if running in a test environment (detected by CODEX_HOME being in /tmp or /var/folders).
pub async fn ensure_default_config() -> Result<()> {
    // Skip in test environments
    if is_test_environment() {
        return Ok(());
    }

    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    let config_path = PathBuf::from(home).join(".osmiflow").join("config.toml");

    if !config_path.exists() {
        create_default_config(&config_path).await?;
    }

    Ok(())
}

fn is_test_environment() -> bool {
    // Check if CODEX_HOME is set to a temp directory (tests do this)
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let path = Path::new(&codex_home);
        // Common temp directory patterns used by tests
        if path.starts_with("/tmp")
            || path.starts_with("/var/folders")
            || path.starts_with("/private/var/folders")
        {
            return true;
        }
    }

    // Check if we're running under cargo test
    if std::env::var("CARGO_TARGET_DIR").is_ok() {
        // This is set during cargo build/test but not during normal execution
        // Combined with the binary name check, this is a good indicator
        if let Ok(current_exe) = std::env::current_exe()
            && let Some(exe_name) = current_exe.file_name()
        {
            let name = exe_name.to_string_lossy();
            // Test binaries often have hashes in their names
            if name.contains('-') && name.len() > 20 {
                return true;
            }
        }
    }

    false
}

async fn create_default_config(config_path: &Path) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create .osmiflow directory")?;
    }

    // Write the default config
    fs::write(config_path, DEFAULT_CONFIG_TOML)
        .await
        .context("Failed to write default config.toml")?;

    Ok(())
}
