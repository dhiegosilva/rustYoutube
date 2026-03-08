mod auth;
mod deps;
mod i18n;
mod player;
mod ui;
mod youtube;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// i18n is initialized lazily when first used

fn credentials_path() -> Option<PathBuf> {
    let dir = dirs::config_dir().or_else(|| dirs::home_dir().map(|d| d.join(".config")))?
        .join("rustyoutube");
    Some(dir.join("credentials.json"))
}

#[derive(Serialize, Deserialize)]
struct StoredCredentials {
    client_id: String,
    client_secret: String,
}

fn load_credentials_from_file() -> Option<(String, String)> {
    let path = credentials_path()?;
    let content = fs::read_to_string(&path).ok()?;
    let stored: StoredCredentials = serde_json::from_str(&content).ok()?;
    if stored.client_id.is_empty() || stored.client_secret.is_empty() {
        return None;
    }
    Some((stored.client_id, stored.client_secret))
}

fn save_credentials_to_file(client_id: &str, client_secret: &str) -> Result<()> {
    let path = credentials_path().ok_or_else(|| anyhow::anyhow!("No config directory"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stored = StoredCredentials {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
    };
    let content = serde_json::to_string_pretty(&stored)?;
    fs::write(path, content)?;
    Ok(())
}

/// Load or prompt for Google OAuth credentials and set them in the process environment.
/// Tries: (1) env vars, (2) saved credentials file, (3) prompt user and save to file.
fn prompt_for_google_credentials_if_needed() {
    if std::env::var("GOOGLE_CLIENT_ID").is_ok() && std::env::var("GOOGLE_CLIENT_SECRET").is_ok() {
        return;
    }
    // Try loading from saved credentials file
    if let Some((id, secret)) = load_credentials_from_file() {
        std::env::set_var("GOOGLE_CLIENT_ID", &id);
        std::env::set_var("GOOGLE_CLIENT_SECRET", &secret);
        return;
    }
    println!("\nGoogle OAuth credentials are required to use YouTube features.");
    println!("Get them at: https://console.cloud.google/ (YouTube Data API v3, OAuth 2.0 \"TVs and Limited Input devices\")");
    if std::env::var("GOOGLE_CLIENT_ID").is_err() {
        print!("Enter GOOGLE_CLIENT_ID: ");
        io::stdout().flush().ok();
        let mut s = String::new();
        if io::stdin().read_line(&mut s).is_ok() {
            let id = s.trim().to_string();
            if !id.is_empty() {
                std::env::set_var("GOOGLE_CLIENT_ID", id);
            }
        }
    }
    if std::env::var("GOOGLE_CLIENT_SECRET").is_err() {
        print!("Enter GOOGLE_CLIENT_SECRET: ");
        io::stdout().flush().ok();
        let mut s = String::new();
        if io::stdin().read_line(&mut s).is_ok() {
            let secret = s.trim().to_string();
            if !secret.is_empty() {
                std::env::set_var("GOOGLE_CLIENT_SECRET", secret);
            }
        }
    }
    // Save to file so user doesn't have to enter again
    if let (Ok(id), Ok(secret)) = (
        std::env::var("GOOGLE_CLIENT_ID"),
        std::env::var("GOOGLE_CLIENT_SECRET"),
    ) {
        if let Err(e) = save_credentials_to_file(&id, &secret) {
            eprintln!("Note: Could not save credentials to file: {}", e);
        } else {
            println!("Credentials saved for future runs.");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Checking dependencies...");

    // Ensure VLC and yt-dlp are installed
    if let Err(e) = deps::ensure_dependencies().await {
        eprintln!("Warning: {}", e);
        eprintln!("The application may not work correctly without these dependencies.");
        eprintln!("Press Enter to continue anyway, or Ctrl+C to exit...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        // Flush stdout to ensure message is displayed
        std::io::stdout().flush().ok();
    }

    // If OAuth env vars are missing, prompt for them so the app can continue
    prompt_for_google_credentials_if_needed();

    // Initialize auth client
    let auth_client = match auth::AuthClient::new() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("\nPlease set the following environment variables (or enter them when prompted):");
            eprintln!("  GOOGLE_CLIENT_ID=your-client-id");
            eprintln!("  GOOGLE_CLIENT_SECRET=your-client-secret");
            eprintln!("\nTo get these credentials:");
            eprintln!("  1. Go to https://console.cloud.google.com/");
            eprintln!("  2. Create a project and enable YouTube Data API v3");
            eprintln!("  3. Create OAuth 2.0 credentials (TVs and Limited Input devices)");
            eprintln!("  4. Set the environment variables or enter them when the app asks.");
            return Err(e);
        }
    };

    // Authenticate (or load existing token)
    println!("Authenticating with YouTube...");
    let access_token = match auth_client.get_access_token().await {
        Ok(token) => {
            println!("✓ Authenticated successfully!");
            token
        }
        Err(e) => {
            eprintln!("Authentication failed: {}", e);
            return Err(e);
        }
    };

    // Initialize YouTube client with authentication
    let http_client = reqwest::Client::new();
    let youtube_client = youtube::YouTubeClient::with_auth(http_client, access_token);

    // Run the UI
    println!("Starting UI...");
    if let Err(e) = ui::run(youtube_client).await {
        eprintln!("Error running UI: {}", e);
        return Err(e);
    }

    Ok(())
}
