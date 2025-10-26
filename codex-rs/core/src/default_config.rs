use std::fs;
use std::io::Write;
use std::path::Path;

const DEFAULT_CONFIG: &str = r#"# Set the default model and provider to use your custom setup
model = "osmi/qwen3-next-80b"
model_provider = "osmi"
preferred_auth_method = "apikey"

# Define the custom model provider settings
[model_providers.osmi]
name = "Gala powered by OSMI"
base_url = "https://models.osmi.ai/v1"
env_key = "OSMI_API_KEY"
wire_api = "chat"
stream = "false"

# Local OpenAI-compatible API provider at http://127.0.0.1:8080
[model_providers.local-openai]
name = "Local OpenAI API"
base_url = "http://127.0.0.1:8080/v1"
wire_api = "chat"
"#;

/// Initialize default config if it doesn't exist
pub fn ensure_default_config(codex_home: &Path) -> std::io::Result<()> {
    let config_path = codex_home.join("config.toml");

    // Check if config.toml already exists
    if config_path.exists() {
        return Ok(());
    }

    // Ensure the codex_home directory exists
    fs::create_dir_all(codex_home)?;

    // Create the config file with default content
    let mut file = fs::File::create(&config_path)?;
    file.write_all(DEFAULT_CONFIG.as_bytes())?;
    file.sync_all()?;

    eprintln!(
        "Created default configuration at: {}",
        config_path.display()
    );

    Ok(())
}
