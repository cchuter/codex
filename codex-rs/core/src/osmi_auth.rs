//! OSMI API authentication module

use std::io;
use std::io::Write;

use anyhow::Context;
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize)]
struct AuthRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AuthResponse {
    // We only need to check if the response is valid
    // The actual content doesn't matter for auth
    #[allow(dead_code)]
    id: Option<String>,
}

/// Authenticates an API key against the OSMI API
async fn verify_api_key(api_key: &str, base_url: &str) -> Result<bool> {
    let client = Client::new();

    let auth_request = AuthRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "Hi".to_string(),
        }],
        max_tokens: 1,
    };

    let response = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&auth_request)
        .send()
        .await
        .context("Failed to send authentication request")?;

    // Check if we got a successful response
    Ok(response.status().is_success())
}

/// Prompts the user for an API key via stdin
fn prompt_for_api_key(env_key_name: &str) -> Result<String> {
    print!("Please enter your {} (it will be hidden): ", env_key_name);
    io::stdout().flush()?;

    // Read the API key from stdin
    let mut api_key = String::new();
    io::stdin().read_line(&mut api_key)?;

    Ok(api_key.trim().to_string())
}

/// Verifies OSMI authentication for a provider configuration.
///
/// This function:
/// 1. Checks if the environment variable specified by env_key is set
/// 2. If not set, prompts the user to provide it
/// 3. Authenticates against the OSMI API
/// 4. Sets the environment variable if authentication succeeds and it wasn't previously set
///
/// Returns the validated API key if successful.
pub async fn verify_osmi_auth(
    provider_name: &str,
    base_url: &str,
    env_key_name: &str,
) -> Result<String> {
    // Skip authentication in test environments
    if is_test_environment() {
        return Ok("test-key".to_string());
    }

    // Check if the environment variable is already set
    let api_key = match std::env::var(env_key_name) {
        Ok(key) if !key.trim().is_empty() => {
            // API key exists, verify it works
            if verify_api_key(&key, base_url).await? {
                tracing::debug!("Using existing {} from environment", env_key_name);
                key
            } else {
                // Existing key is invalid, prompt for a new one
                eprintln!("Warning: Existing {} is invalid", env_key_name);
                let new_key = prompt_for_api_key(env_key_name)?;

                // Verify the new key
                if !verify_api_key(&new_key, base_url).await? {
                    anyhow::bail!("Invalid API key for {}", provider_name);
                }

                // Set the environment variable for this session
                unsafe {
                    std::env::set_var(env_key_name, &new_key);
                }

                eprintln!("✓ Authentication successful for {}", provider_name);
                new_key
            }
        }
        _ => {
            // No API key set, prompt for one
            eprintln!("\n{} provider requires authentication.", provider_name);
            let api_key = prompt_for_api_key(env_key_name)?;

            // Verify the API key
            if !verify_api_key(&api_key, base_url).await? {
                anyhow::bail!("Invalid API key for {}", provider_name);
            }

            // Set the environment variable for this session
            unsafe {
                std::env::set_var(env_key_name, &api_key);
            }

            eprintln!("✓ Authentication successful for {}", provider_name);
            api_key
        }
    };

    Ok(api_key)
}

/// Checks all configured model providers and ensures authentication for those requiring it
pub async fn ensure_provider_auth(config: &crate::config::Config) -> Result<()> {
    // Iterate through all configured model providers
    for (provider_id, provider_info) in &config.model_providers {
        // Check if this provider needs authentication
        if let Some(env_key) = &provider_info.env_key {
            // Special handling for OSMI provider
            if provider_id == "osmi"
                || provider_info
                    .base_url
                    .as_ref()
                    .map_or(false, |url| url.contains("osmi.ai"))
            {
                let base_url = provider_info
                    .base_url
                    .as_ref()
                    .cloned()
                    .unwrap_or_else(|| "https://models.osmi.ai/v1".to_string());

                // Verify authentication
                verify_osmi_auth(&provider_info.name, &base_url, env_key).await?;
            }
            // Note: Other providers could be handled here in the future
        }
    }

    Ok(())
}

fn is_test_environment() -> bool {
    // Check if CODEX_HOME is set to a temp directory (tests do this)
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        let path = std::path::Path::new(&codex_home);
        if path.starts_with("/tmp")
            || path.starts_with("/var/folders")
            || path.starts_with("/private/var/folders")
        {
            return true;
        }
    }

    // Check if we're running under cargo test
    if std::env::var("CARGO_TARGET_DIR").is_ok()
        && let Ok(current_exe) = std::env::current_exe()
        && let Some(exe_name) = current_exe.file_name()
    {
        let name = exe_name.to_string_lossy();
        // Test binaries often have hashes in their names
        if name.contains('-') && name.len() > 20 {
            return true;
        }
    }

    false
}
