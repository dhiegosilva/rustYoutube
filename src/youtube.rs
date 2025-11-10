use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use tokio::process::Command as TokioCommand;

#[derive(Debug, Clone)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel_title: String,
    pub published_at: String,
    pub thumbnail_url: String,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub channel_id: String,
    pub channel_title: String,
    pub thumbnail_url: String,
}

#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub description: String,
    pub item_count: u32,
}

pub struct YouTubeClient {
    client: Option<Client>,
    access_token: Option<String>,
}

impl YouTubeClient {

    pub fn with_auth(client: Client, access_token: String) -> Self {
        Self {
            client: Some(client),
            access_token: Some(access_token),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.client.is_some() && self.access_token.is_some()
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

    // Get subscriptions (requires authentication)
    pub async fn get_subscriptions(&self) -> Result<Vec<Subscription>> {
        let client = self.client.as_ref().context("Not authenticated")?;
        let token = self.access_token.as_ref().context("Not authenticated")?;

        let mut subscriptions = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = "https://www.googleapis.com/youtube/v3/subscriptions?part=snippet&mine=true&maxResults=50".to_string();
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", token));
            }

            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow::anyhow!("API error: {}", error_text));
            }

            let response_text = response.text().await?;
            let data: SubscriptionResponse = serde_json::from_str(&response_text)
                .map_err(|e| anyhow::anyhow!("Failed to parse subscription response: {}\nResponse: {}", e, response_text))?;

            let items_count = data.items.len();
            for item in data.items {
                if let Some(resource_id) = &item.snippet.resource_id {
                    subscriptions.push(Subscription {
                        channel_id: resource_id.channel_id.clone(),
                        channel_title: item.snippet.title.clone(),
                        thumbnail_url: item.snippet.thumbnails.default.url.clone(),
                    });
                } else {
                    // Log warning for items without resource_id
                    eprintln!("Warning: Subscription item missing resource_id: {:?}", item.snippet.title);
                }
            }
            
            // If we got items but none had resource_id, log warning
            if items_count > 0 && subscriptions.is_empty() {
                eprintln!("Warning: Received {} subscription items but none had resource_id", items_count);
            }

            page_token = data.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(subscriptions)
    }


    // Get playlists
    pub async fn get_playlists(&self) -> Result<Vec<Playlist>> {
        let client = self.client.as_ref().context("Not authenticated")?;
        let token = self.access_token.as_ref().context("Not authenticated")?;

        let mut playlists = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = "https://www.googleapis.com/youtube/v3/playlists?part=snippet,contentDetails&mine=true&maxResults=50".to_string();
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", token));
            }

            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow::anyhow!("API error: {}", error_text));
            }

            let response_text = response.text().await?;
            let data: PlaylistResponse = serde_json::from_str(&response_text)
                .map_err(|e| anyhow::anyhow!("Failed to parse playlist response: {}\nResponse: {}", e, response_text))?;

            for item in data.items {
                let item_count = item.content_details
                    .as_ref()
                    .map(|cd| cd.item_count)
                    .unwrap_or(0);
                playlists.push(Playlist {
                    id: item.id,
                    title: item.snippet.title,
                    description: item.snippet.description,
                    item_count,
                });
            }

            page_token = data.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(playlists)
    }

    // Get videos from a playlist
    pub async fn get_playlist_videos(&self, playlist_id: &str) -> Result<Vec<Video>> {
        let client = self.client.as_ref().context("Not authenticated")?;
        let token = self.access_token.as_ref().context("Not authenticated")?;

        let mut videos = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("https://www.googleapis.com/youtube/v3/playlistItems?part=snippet,contentDetails&playlistId={}&maxResults=50", playlist_id);
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", token));
            }

            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow::anyhow!("API error: {}", error_text));
            }

            let response_text = response.text().await?;
            let data: PlaylistItemsResponse = serde_json::from_str(&response_text)
                .map_err(|e| anyhow::anyhow!("Failed to parse playlist items response: {}\nResponse: {}", e, response_text))?;

            for item in data.items {
                // Try to get video ID from content_details first, then from resourceId
                let video_id = if let Some(content_details) = &item.content_details {
                    Some(content_details.video_id.clone())
                } else if let Some(resource_id) = &item.snippet.resource_id {
                    resource_id.video_id.clone()
                } else {
                    None
                };
                
                if let Some(vid_id) = video_id {
                    videos.push(Video {
                        id: vid_id,
                        title: item.snippet.title.clone(),
                        channel_title: item.snippet.channel_title.clone().unwrap_or_else(|| "Unknown Channel".to_string()),
                        published_at: item.snippet.published_at.clone().unwrap_or_else(|| "Unknown date".to_string()),
                        thumbnail_url: item.snippet.thumbnails.default.url.clone(),
                    });
                } else {
                    // If we can't get video ID, skip this item
                    eprintln!("Warning: Playlist item missing video ID, skipping: {}", item.snippet.title);
                }
            }

            page_token = data.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(videos)
    }

    // Helper: Get channel videos by channel ID (using API)
    pub async fn get_channel_videos_by_id(&self, channel_id: &str) -> Result<Vec<Video>> {
        let client = self.client.as_ref().context("Not authenticated")?;
        let token = self.access_token.as_ref().context("Not authenticated")?;

        // Get uploads playlist ID
        let url = format!("https://www.googleapis.com/youtube/v3/channels?part=contentDetails&id={}", channel_id);
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to get channel info"));
        }

        let data: serde_json::Value = response.json().await?;
        let uploads_playlist_id = data["items"][0]["contentDetails"]["relatedPlaylists"]["uploads"]
            .as_str()
            .context("No uploads playlist found")?;

        // Get videos from uploads playlist
        self.get_playlist_videos(uploads_playlist_id).await
    }

    // Get channel playlists by channel ID
    pub async fn get_channel_playlists(&self, channel_id: &str) -> Result<Vec<Playlist>> {
        let client = self.client.as_ref().context("Not authenticated")?;
        let token = self.access_token.as_ref().context("Not authenticated")?;

        let mut playlists = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = format!("https://www.googleapis.com/youtube/v3/playlists?part=snippet,contentDetails&channelId={}&maxResults=50", channel_id);
            if let Some(token) = &page_token {
                url.push_str(&format!("&pageToken={}", token));
            }

            let response = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?;

            if !response.status().is_success() {
                let error_text = response.text().await?;
                return Err(anyhow::anyhow!("API error: {}", error_text));
            }

            let response_text = response.text().await?;
            let data: PlaylistResponse = serde_json::from_str(&response_text)
                .map_err(|e| anyhow::anyhow!("Failed to parse playlist response: {}\nResponse: {}", e, response_text))?;

            for item in data.items {
                let item_count = item.content_details
                    .as_ref()
                    .map(|cd| cd.item_count)
                    .unwrap_or(0);
                playlists.push(Playlist {
                    id: item.id,
                    title: item.snippet.title,
                    description: item.snippet.description,
                    item_count,
                });
            }

            page_token = data.next_page_token;
            if page_token.is_none() {
                break;
            }
        }

        Ok(playlists)
    }
}

// API Response structures
#[derive(Deserialize)]
struct SubscriptionResponse {
    items: Vec<SubscriptionItem>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct SubscriptionItem {
    snippet: SubscriptionSnippet,
}

#[derive(Deserialize)]
struct SubscriptionSnippet {
    title: String,
    #[serde(rename = "resourceId", default)]
    resource_id: Option<ResourceId>,
    #[serde(default)]
    thumbnails: Thumbnails,
}

#[derive(Deserialize)]
struct ResourceId {
    #[serde(rename = "channelId")]
    channel_id: String,
}

#[derive(Deserialize)]
struct PlaylistResponse {
    items: Vec<PlaylistItem>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistItem {
    id: String,
    snippet: PlaylistSnippet,
    #[serde(default, rename = "contentDetails")]
    content_details: Option<PlaylistContentDetails>,
}

#[derive(Deserialize)]
struct PlaylistSnippet {
    title: String,
    description: String,
}

#[derive(Deserialize)]
struct PlaylistContentDetails {
    #[serde(rename = "itemCount")]
    item_count: u32,
}

#[derive(Deserialize)]
struct PlaylistItemsResponse {
    items: Vec<PlaylistVideoItem>,
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistVideoItem {
    snippet: PlaylistVideoSnippet,
    #[serde(default)]
    content_details: Option<PlaylistVideoContentDetails>,
}

#[derive(Deserialize)]
struct PlaylistVideoSnippet {
    title: String,
    #[serde(default)]
    channel_title: Option<String>,
    #[serde(default, rename = "publishedAt")]
    published_at: Option<String>,
    #[serde(default)]
    thumbnails: Thumbnails,
    #[serde(default, rename = "resourceId")]
    resource_id: Option<PlaylistResourceId>,
}

#[derive(Deserialize)]
struct PlaylistResourceId {
    #[serde(rename = "videoId")]
    video_id: Option<String>,
}

#[derive(Deserialize)]
struct PlaylistVideoContentDetails {
    video_id: String,
}

#[derive(Deserialize, Default)]
struct Thumbnails {
    #[serde(default)]
    default: Thumbnail,
}

#[derive(Deserialize, Default)]
struct Thumbnail {
    #[serde(default)]
    url: String,
}
