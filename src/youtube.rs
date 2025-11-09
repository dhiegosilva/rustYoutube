use anyhow::Result;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel_title: String,
    pub published_at: String,
    pub thumbnail_url: String,
}

pub struct YouTubeClient;

impl YouTubeClient {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_channel_videos(&self, channel_url: &str) -> Result<Vec<Video>> {
        use crate::deps;
        
        // Check if yt-dlp is available, try to install if not
        if !deps::check_ytdlp().await {
            println!("yt-dlp not found. Attempting to install...");
            if let Err(e) = deps::ensure_ytdlp().await {
                return Err(anyhow::anyhow!(
                    "yt-dlp is not installed and auto-installation failed: {}\n\
                    Please install it manually:\n\
                    Windows: winget install yt-dlp.yt-dlp",
                    e
                ));
            }
            // Verify it's now available
            if !deps::check_ytdlp().await {
                return Err(anyhow::anyhow!(
                    "yt-dlp installation completed but it's still not available.\n\
                    Please restart the program or install manually:\n\
                    Windows: winget install yt-dlp.yt-dlp"
                ));
            }
        }
        
        // Use yt-dlp to get channel videos - prefer local version if available
        #[cfg(windows)]
        let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
            local_ytdlp.to_str().unwrap().to_string()
        } else {
            "yt-dlp.exe".to_string()
        };
        #[cfg(not(windows))]
        let ytdlp_cmd = "yt-dlp";
        
        // Normalize the URL - ensure it's a full YouTube URL
        let normalized_url = if channel_url.starts_with("http") {
            channel_url.to_string()
        } else if channel_url.starts_with("@") {
            format!("https://www.youtube.com/{}/videos", channel_url)
        } else {
            format!("https://www.youtube.com/{}", channel_url)
        };
        
        println!("Fetching videos from: {}", normalized_url);
        
        // Use yt-dlp to get channel videos
        let output = TokioCommand::new(ytdlp_cmd)
            .args(&[
                "--flat-playlist",
                "--print", "%(id)s|%(title)s|%(uploader)s|%(upload_date)s",
                "--playlist-end", "20", // Get top 20 videos
                &normalized_url,
            ])
            .output()
            .await;

        let output = match output {
            Ok(output) => output,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    return Err(anyhow::anyhow!(
                        "yt-dlp not found. Please install it:\n\
                        Windows: winget install yt-dlp\n\
                        Or the program will try to install it automatically on next run."
                    ));
                }
                return Err(anyhow::anyhow!("Failed to run yt-dlp: {}", e));
            }
        };

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Check if it's a "not found" error
            if error.contains("not found") || error.contains("not recognized") || error.is_empty() {
                return Err(anyhow::anyhow!(
                    "yt-dlp is not installed or not in PATH.\n\
                    Please install it:\n\
                    Windows: winget install yt-dlp\n\
                    Or run the program again to auto-install."
                ));
            }
            
            return Err(anyhow::anyhow!(
                "Failed to get channel videos.\nError: {}\nOutput: {}",
                error,
                stdout
            ));
        }

        self.parse_ytdlp_output(&output.stdout).await
    }



    async fn parse_ytdlp_output(&self, output: &[u8]) -> Result<Vec<Video>> {
        let mut videos = Vec::new();
        let output_str = String::from_utf8_lossy(output);
        
        for line in output_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() >= 4 {
                let id = parts[0].to_string();
                let title = parts[1].to_string();
                let uploader = parts[2].to_string();
                let upload_date = parts.get(3).unwrap_or(&"").to_string();
                
                // Format date
                let formatted_date = if upload_date.len() >= 8 {
                    format!("{}-{}-{}", &upload_date[0..4], &upload_date[4..6], &upload_date[6..8])
                } else {
                    upload_date
                };
                
                videos.push(Video {
                    id,
                    title,
                    channel_title: uploader,
                    published_at: formatted_date,
                    thumbnail_url: String::new(),
                });
            }
        }
        
        Ok(videos)
    }
}
