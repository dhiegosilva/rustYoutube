mod auth;
mod deps;
mod i18n;
mod player;
mod ui;
mod youtube;

use anyhow::Result;

// i18n is initialized lazily when first used

#[tokio::main]
async fn main() -> Result<()> {
    println!("Checking dependencies...");

    // Ensure mpv and yt-dlp are installed
    if let Err(e) = deps::ensure_dependencies().await {
        eprintln!("Warning: {}", e);
        eprintln!("The application may not work correctly without these dependencies.");
        eprintln!("Press Enter to continue anyway, or Ctrl+C to exit...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        // Flush stdout to ensure message is displayed
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    // Initialize auth client
    let auth_client = match auth::AuthClient::new() {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Error: {}", e);
            eprintln!("\nPlease set the following environment variables:");
            eprintln!("  GOOGLE_CLIENT_ID=your-client-id");
            eprintln!("  GOOGLE_CLIENT_SECRET=your-client-secret");
            eprintln!("\nTo get these credentials:");
            eprintln!("  1. Go to https://console.cloud.google.com/");
            eprintln!("  2. Create a project and enable YouTube Data API v3");
            eprintln!("  3. Create OAuth 2.0 credentials (TVs and Limited Input devices)");
            eprintln!("  4. Set the environment variables");
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
