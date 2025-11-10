use anyhow::{Context, Result};
use dirs;
use oauth2::{ClientId, ClientSecret};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

pub struct AuthClient {
    client_id: ClientId,
    client_secret: ClientSecret,
    token_path: PathBuf,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    #[serde(default)]
    verification_url_complete: Option<String>,
    expires_in: u64,
    interval: u64,
}

#[derive(Deserialize)]
struct TokenPollResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
}

impl AuthClient {
    pub fn new() -> Result<Self> {
        // Get OAuth credentials from environment
        let client_id = std::env::var("GOOGLE_CLIENT_ID")
            .context("GOOGLE_CLIENT_ID environment variable not set. Please set it before running.")?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")
            .context("GOOGLE_CLIENT_SECRET environment variable not set. Please set it before running.")?;

        let config_dir = get_config_dir()?;
        let token_path = config_dir.join("token.json");

        Ok(Self {
            client_id: ClientId::new(client_id),
            client_secret: ClientSecret::new(client_secret),
            token_path,
        })
    }

    pub async fn authenticate(&self) -> Result<String> {
        // Check if we have a valid token
        if let Ok(token) = self.load_token().await {
            if self.is_token_valid(&token).await {
                return Ok(token.access_token);
            }
        }

        // Start Device Authorization Flow
        println!("\n=== YouTube Authentication ===");
        println!("Starting device authorization flow...\n");

        let http_client = Client::new();
        
        // Step 1: Request device code
        let device_code_response = http_client
            .post("https://oauth2.googleapis.com/device/code")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("scope", "https://www.googleapis.com/auth/youtube.readonly"),
            ])
            .send()
            .await
            .context("Failed to request device code")?;

        if !device_code_response.status().is_success() {
            let error = device_code_response.text().await?;
            return Err(anyhow::anyhow!("Failed to get device code: {}", error));
        }

        let device_data: DeviceCodeResponse = device_code_response.json().await?;

        // Step 2: Display instructions to user
        println!("┌─────────────────────────────────────────────────────────┐");
        println!("│  Visit this URL on any device:                           │");
        println!("│  {}", device_data.verification_url);
        println!("│                                                           │");
        println!("│  Enter this code: {}", device_data.user_code);
        if let Some(complete_url) = &device_data.verification_url_complete {
            println!("│                                                           │");
            println!("│  Or visit this complete URL:                             │");
            println!("│  {}", complete_url);
        }
        println!("└─────────────────────────────────────────────────────────┘");
        println!("\nWaiting for authorization... (Press Ctrl+C to cancel)\n");

        // Step 3: Poll for token
        let poll_interval = Duration::from_secs(device_data.interval);
        let expires_at = std::time::Instant::now() + Duration::from_secs(device_data.expires_in);

        loop {
            if std::time::Instant::now() > expires_at {
                return Err(anyhow::anyhow!("Device code expired. Please try again."));
            }

            tokio::time::sleep(poll_interval).await;

            let token_response = http_client
                .post("https://oauth2.googleapis.com/token")
                .form(&[
                    ("client_id", self.client_id.as_str()),
                    ("client_secret", self.client_secret.secret()),
                    ("device_code", &device_data.device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ])
                .send()
                .await?;

            let token_data: TokenPollResponse = token_response.json().await?;

            if let Some(error) = &token_data.error {
                match error.as_str() {
                    "authorization_pending" => {
                        print!(".");
                        use std::io::Write;
                        std::io::stdout().flush().ok();
                        continue;
                    }
                    "slow_down" => {
                        tokio::time::sleep(poll_interval * 2).await;
                        continue;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("Authorization error: {}", error));
                    }
                }
            }

            if let Some(access_token) = token_data.access_token {
                let refresh_token = token_data.refresh_token;
                let expires_at = token_data.expires_in.map(|d| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs() + d
                });

                let token_data = TokenData {
                    access_token: access_token.clone(),
                    refresh_token,
                    expires_at,
                };

                self.save_token(&token_data).await?;
                println!("\n✓ Authentication successful!\n");
                return Ok(access_token);
            }
        }
    }

    async fn load_token(&self) -> Result<TokenData> {
        let content = fs::read_to_string(&self.token_path)
            .context("Failed to read token file")?;
        let token: TokenData = serde_json::from_str(&content)
            .context("Failed to parse token file")?;
        Ok(token)
    }

    async fn save_token(&self, token: &TokenData) -> Result<()> {
        if let Some(parent) = self.token_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(token)?;
        fs::write(&self.token_path, content)?;
        Ok(())
    }

    async fn is_token_valid(&self, token: &TokenData) -> bool {
        if let Some(expires_at) = token.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now >= expires_at {
                return false;
            }
        }

        // Test token by making a simple API call
        let client = Client::new();
        let response = client
            .get("https://www.googleapis.com/youtube/v3/channels?part=snippet&mine=true")
            .bearer_auth(&token.access_token)
            .send()
            .await;

        matches!(response, Ok(r) if r.status().is_success())
    }

    pub async fn get_access_token(&self) -> Result<String> {
        if let Ok(token) = self.load_token().await {
            if self.is_token_valid(&token).await {
                return Ok(token.access_token);
            }

            // Try to refresh token
            if let Some(refresh_token) = &token.refresh_token {
                if let Ok(new_token) = self.refresh_token(refresh_token).await {
                    return Ok(new_token.access_token);
                }
            }
        }

        // Re-authenticate
        self.authenticate().await
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<TokenData> {
        let client = Client::new();
        let response = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.secret()),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Failed to refresh token: {}", error));
        }

        let data: serde_json::Value = response.json().await?;
        let access_token = data["access_token"]
            .as_str()
            .context("No access token in response")?
            .to_string();

        let token_data = TokenData {
            access_token,
            refresh_token: Some(refresh_token.to_string()),
            expires_at: data["expires_in"].as_u64().map(|d| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() + d
            }),
        };

        self.save_token(&token_data).await?;
        Ok(token_data)
    }
}

fn get_config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|d| d.join(".config")))
        .context("Failed to find config directory")?
        .join("rustyoutube");
    Ok(dir)
}

