// Allow prints in this module since it's for user interaction during startup
#![allow(clippy::print_stdout, clippy::print_stderr)]

use crate::error::CodexErr;
use crate::error::Result;
use reqwest;
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
use std::time::Duration;

const OSMI_API_BASE_URL: &str = "https://models.osmi.ai";

#[derive(Debug, Serialize, Deserialize)]
struct OsmiAuth {
    api_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthTestRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
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
fn save_osmi_api_key(codex_home: &Path, api_key: &str) -> io::Result<()> {
    let auth = OsmiAuth {
        api_key: api_key.to_string(),
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

/// Authenticate API key against models.osmi.ai
async fn authenticate_api_key(api_key: &str) -> Result<bool> {
    // Test the API key by making a minimal request to the API
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| CodexErr::Io(io::Error::other(e)))?;

    let test_request = AuthTestRequest {
        model: "osmi/qwen3-next-80b".to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: "test".to_string(),
        }],
        max_tokens: 1,
    };

    let response = client
        .post(format!("{OSMI_API_BASE_URL}/v1/chat/completions"))
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&test_request)
        .send()
        .await
        .map_err(|e| {
            CodexErr::Io(io::Error::other(format!(
                "Failed to connect to OSMI API: {e}"
            )))
        })?;

    // Check if authentication was successful
    match response.status() {
        reqwest::StatusCode::OK => Ok(true),
        reqwest::StatusCode::UNAUTHORIZED => Ok(false),
        status => {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(CodexErr::Io(io::Error::other(format!(
                "OSMI API error ({status}): {error_text}"
            ))))
        }
    }
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

    // Ensure the API key has the correct format
    if !trimmed.starts_with("osmi-") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid API key format. OSMI API keys should start with 'osmi-'",
        ));
    }

    Ok(trimmed)
}

/// Check if OSMI is configured as the model provider
pub fn is_osmi_provider(config: &crate::config::Config) -> bool {
    config.model_provider_id == "osmi" || config.model.starts_with("osmi/")
}

/// Verify and prompt for OSMI API key if needed
pub async fn verify_and_prompt_osmi_api_key(codex_home: &Path) -> Result<()> {
    // Check if OSMI_API_KEY environment variable is already set
    if let Ok(api_key) = std::env::var("OSMI_API_KEY") && !api_key.trim().is_empty() {
        eprintln!("ℹ️  Using OSMI API key from environment variable.");

        // Authenticate the API key
        eprintln!("🔐 Authenticating with models.osmi.ai...");
        match authenticate_api_key(&api_key).await {
            Ok(true) => {
                eprintln!("✅ Authentication successful!");
                return Ok(());
            }
            Ok(false) => {
                eprintln!("❌ Authentication failed: Invalid API key.");
                eprintln!("   Please check your OSMI_API_KEY environment variable.");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("❌ Authentication error: {e}");
                std::process::exit(1);
            }
        }
    }

    // Check if we have a saved API key
    if let Some(saved_api_key) = load_osmi_api_key(codex_home) {
        // Set it as an environment variable for this session
        // SAFETY: Setting environment variables is safe in single-threaded context at startup
        unsafe {
            std::env::set_var("OSMI_API_KEY", &saved_api_key);
        }

        eprintln!("ℹ️  Loaded OSMI API key from saved configuration.");

        // Authenticate the saved API key
        eprintln!("🔐 Authenticating with models.osmi.ai...");
        match authenticate_api_key(&saved_api_key).await {
            Ok(true) => {
                eprintln!("✅ Authentication successful!");
                return Ok(());
            }
            Ok(false) => {
                eprintln!("⚠️  Saved API key is no longer valid. Please enter a new one.");
                // Continue to prompt for a new key
            }
            Err(e) => {
                eprintln!("⚠️  Authentication error with saved key: {e}");
                // Continue to prompt for a new key
            }
        }
    }

    // Prompt user for API key
    eprintln!("\n⚠️  OSMI API Key not found!");
    eprintln!("The OSMI provider requires an API key to function.");
    eprintln!("You can get your API key from https://models.osmi.ai\n");

    let api_key = prompt_for_api_key()
        .map_err(|e| CodexErr::Io(io::Error::other(format!("Failed to read API key: {e}"))))?;

    // Authenticate the API key
    eprintln!("\n🔐 Authenticating with models.osmi.ai...");
    match authenticate_api_key(&api_key).await {
        Ok(true) => {
            eprintln!("✅ Authentication successful!");
        }
        Ok(false) => {
            eprintln!("❌ Authentication failed: Invalid API key.");
            eprintln!("   Please check your API key and try again.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("❌ Authentication error: {e}");
            std::process::exit(1);
        }
    }

    // Set the environment variable for this session
    // SAFETY: Setting environment variables is safe in single-threaded context at startup
    unsafe {
        std::env::set_var("OSMI_API_KEY", &api_key);
    }

    // Ask if user wants to save the API key
    print!("\nWould you like to save this API key for future sessions? (y/n): ");
    io::stdout().flush().map_err(CodexErr::Io)?;

    let mut response = String::new();
    io::stdin().read_line(&mut response).map_err(CodexErr::Io)?;

    if response.trim().to_lowercase() == "y" {
        save_osmi_api_key(codex_home, &api_key).map_err(CodexErr::Io)?;
        eprintln!("\n✅ API key saved successfully!");
        eprintln!("   The key has been saved to ~/.osmiflow/osmi_auth.json");
        eprintln!("   It will be automatically loaded and authenticated in future sessions.");
        eprintln!("\n   For other applications, you can also set it as an environment variable:");
        eprintln!("   export OSMI_API_KEY=\"{api_key}\"");
    } else {
        eprintln!("\nℹ️  API key not saved. It is set for this session only.");
        eprintln!("   To make it permanent, add to your shell profile:");
        eprintln!("   export OSMI_API_KEY=\"{api_key}\"");
    }

    Ok(())
}
