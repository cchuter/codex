// Allow prints in this module since it's for user interaction during startup
#![allow(clippy::print_stdout, clippy::print_stderr)]

use crate::config_edit::persist_overrides;
use crate::error::CodexErr;
use crate::error::Result;
use serde::Deserialize;
use serde::Serialize;
use std::fs::OpenOptions;
use std::fs::{self};
use std::io::Write;
use std::io::{self};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
struct OsmiAuth {
    api_key: String,
    env_key: String,
}

/// Get the path to the OSMI auth file
fn get_osmi_auth_file(codex_home: &Path) -> PathBuf {
    codex_home.join("osmi_auth.json")
}

/// Load OSMI API key from file if it exists
fn load_osmi_api_key(codex_home: &Path) -> Option<String> {
    let auth_file = get_osmi_auth_file(codex_home);
    if !auth_file.exists() {
        return None;
    }

    let contents = fs::read_to_string(&auth_file).ok()?;
    let auth: OsmiAuth = serde_json::from_str(&contents).ok()?;
    Some(auth.api_key)
}

/// Save OSMI API key to file
fn save_osmi_api_key(codex_home: &Path, api_key: &str, env_key: &str) -> io::Result<()> {
    let auth = OsmiAuth {
        api_key: api_key.to_string(),
        env_key: env_key.to_string(),
    };

    let auth_file = get_osmi_auth_file(codex_home);
    let json = serde_json::to_string_pretty(&auth)?;

    let mut file_opts = OpenOptions::new();
    file_opts.create(true).write(true).truncate(true);

    #[cfg(unix)]
    file_opts.mode(0o600);

    let mut file = file_opts.open(&auth_file)?;
    file.write_all(json.as_bytes())?;
    file.sync_all()?;

    Ok(())
}

/// Check if OSMI API key is configured for the provider
pub fn check_osmi_api_key(env_key: &str) -> bool {
    std::env::var(env_key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Prompt user for OSMI API key interactively
pub fn prompt_for_api_key() -> io::Result<String> {
    print!("Please enter your OSMI API Key: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let trimmed = input.trim().to_string();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "API key cannot be empty",
        ));
    }

    Ok(trimmed)
}

/// Save OSMI API key to config.toml
pub async fn save_osmi_api_key_to_config(
    codex_home: &Path,
    env_key_name: &str,
) -> anyhow::Result<()> {
    // Update the model_providers.osmi.env_key in config.toml
    let overrides = vec![(&["model_providers", "osmi", "env_key"][..], env_key_name)];

    persist_overrides(codex_home, None, &overrides).await?;
    Ok(())
}

/// Verify and prompt for OSMI API key if needed
pub async fn verify_and_prompt_osmi_api_key(
    codex_home: &Path,
    provider_id: &str,
    env_key: Option<&str>,
) -> Result<()> {
    // Only handle OSMI provider
    if provider_id != "osmi" {
        return Ok(());
    }

    let env_key_name = env_key.unwrap_or("OSMI_API_KEY");

    // Check if the API key is already set in environment
    if check_osmi_api_key(env_key_name) {
        return Ok(());
    }

    // Also check OSMI_API_KEY if it's different from the configured env_key
    if env_key_name != "OSMI_API_KEY" && check_osmi_api_key("OSMI_API_KEY") {
        // Copy the value from OSMI_API_KEY to the configured env_key
        if let Ok(api_key) = std::env::var("OSMI_API_KEY") {
            // SAFETY: Setting environment variables is safe in single-threaded context
            unsafe {
                std::env::set_var(env_key_name, api_key);
            }
            return Ok(());
        }
    }

    // Check if we have a saved API key
    if let Some(saved_api_key) = load_osmi_api_key(codex_home) {
        // SAFETY: Setting environment variables is safe in single-threaded context
        unsafe {
            std::env::set_var("OSMI_API_KEY", &saved_api_key);
            if env_key_name != "OSMI_API_KEY" {
                std::env::set_var(env_key_name, &saved_api_key);
            }
        }
        eprintln!("ℹ️  Loaded OSMI API key from saved configuration.");
        return Ok(());
    }

    // Prompt user for API key
    eprintln!("\n⚠️  OSMI API Key not found!");
    eprintln!("The OSMI provider requires an API key to function.");
    eprintln!("You can get your API key from https://models.osmi.ai\n");

    let api_key = prompt_for_api_key().map_err(|e| {
        CodexErr::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to read API key: {e}"),
        ))
    })?;

    // Set the environment variable for this session
    // SAFETY: Setting environment variables is safe in single-threaded context
    unsafe {
        std::env::set_var("OSMI_API_KEY", &api_key);
        if env_key_name != "OSMI_API_KEY" {
            std::env::set_var(env_key_name, &api_key);
        }
    }

    // Optionally save to config
    print!("\nWould you like to save this API key for future sessions? (y/n): ");
    io::stdout().flush().map_err(CodexErr::Io)?;

    let mut response = String::new();
    io::stdin().read_line(&mut response).map_err(CodexErr::Io)?;

    if response.trim().to_lowercase() == "y" {
        // Save the API key to osmi_auth.json
        save_osmi_api_key(codex_home, &api_key, "OSMI_API_KEY").map_err(CodexErr::Io)?;

        // Also update config.toml to use OSMI_API_KEY
        save_osmi_api_key_to_config(codex_home, "OSMI_API_KEY")
            .await
            .map_err(|e| CodexErr::Io(io::Error::new(io::ErrorKind::Other, e)))?;

        eprintln!("\n✅ API key saved successfully!");
        eprintln!("   The key has been saved to ~/.osmiflow/osmi_auth.json");
        eprintln!("   It will be automatically loaded in future sessions.");
        eprintln!("\n   For other applications, you can also set it as an environment variable:");
        eprintln!("   export OSMI_API_KEY=\"{api_key}\"");
    } else {
        eprintln!("\nℹ️  API key not saved. It is set for this session only.");
        eprintln!("   To make it permanent, add to your shell profile:");
        eprintln!("   export OSMI_API_KEY=\"{api_key}\"");
    }

    Ok(())
}
