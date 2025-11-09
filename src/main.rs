mod youtube;
mod ui;
mod player;
mod deps;

use anyhow::Result;

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
    
    // Initialize YouTube client (no auth needed)
    let youtube_client = youtube::YouTubeClient::new();
    
    // Run the UI
    println!("Starting UI...");
    if let Err(e) = ui::run(youtube_client).await {
        eprintln!("Error running UI: {}", e);
        return Err(e);
    }
    
    Ok(())
}

