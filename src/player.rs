use crate::deps;
use anyhow::Result;
use tokio::process::Command as TokioCommand;

pub async fn play_video(video_id: &str) -> Result<()> {
    // Ensure dependencies are available before playing
    if !deps::check_mpv().await {
        deps::ensure_mpv().await?;
    }
    if !deps::check_ytdlp().await {
        deps::ensure_ytdlp().await?;
    }
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    // Use yt-dlp to get the best video URL and pipe it to mpv
    // This approach uses yt-dlp's --format option to get the best quality
    // and pipes it directly to mpv
    
    // Use local yt-dlp if available
    #[cfg(windows)]
    let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
        local_ytdlp.to_str().unwrap().to_string()
    } else {
        "yt-dlp.exe".to_string()
    };
    #[cfg(not(windows))]
    let ytdlp_cmd = "yt-dlp";
    
    let status = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg("best[height<=1080]")
        .arg("--get-url")
        .arg(&url)
        .output()
        .await?;

    if !status.status.success() {
        // Fallback: let yt-dlp handle everything and pipe to mpv
        return play_video_fallback(&url).await;
    }

    let video_url = String::from_utf8_lossy(&status.stdout).trim().to_string();
    
    if video_url.is_empty() {
        return play_video_fallback(&url).await;
    }

    // Play with mpv - use local mpv if available
    #[cfg(windows)]
    let mpv_cmd = if let Some(local_mpv) = deps::get_mpv_path().await {
        local_mpv.to_str().unwrap().to_string()
    } else {
        "mpv.exe".to_string()
    };
    #[cfg(not(windows))]
    let mpv_cmd = "mpv";
    
    let mut mpv = TokioCommand::new(&mpv_cmd)
        .arg(&video_url)
        .arg("--no-terminal")
        .spawn()?;

    mpv.wait().await?;
    Ok(())
}

async fn play_video_fallback(url: &str) -> Result<()> {
    // Fallback: Use yt-dlp to get URL and play with mpv
    // This is simpler and more reliable
    
    // Ensure dependencies are available
    if !deps::check_mpv().await {
        deps::ensure_mpv().await?;
    }
    if !deps::check_ytdlp().await {
        deps::ensure_ytdlp().await?;
    }
    
        // Get the best video URL using yt-dlp - use local version if available
        #[cfg(windows)]
        let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
            local_ytdlp.to_str().unwrap().to_string()
        } else {
            "yt-dlp.exe".to_string()
        };
        #[cfg(not(windows))]
        let ytdlp_cmd = "yt-dlp";
        
        let output = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg("best[height<=1080]/best")
        .arg("--get-url")
        .arg(url)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to get video URL from yt-dlp"));
    }

    let video_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if video_url.is_empty() {
        return Err(anyhow::anyhow!("Empty video URL from yt-dlp"));
    }

    // Play with mpv using the URL - use local mpv if available
    #[cfg(windows)]
    let mpv_cmd = if let Some(local_mpv) = deps::get_mpv_path().await {
        local_mpv.to_str().unwrap().to_string()
    } else {
        "mpv.exe".to_string()
    };
    #[cfg(not(windows))]
    let mpv_cmd = "mpv";
    
    let mut mpv = TokioCommand::new(&mpv_cmd)
        .arg(&video_url)
        .arg("--no-terminal")
        .spawn()?;

    mpv.wait().await?;
    
    Ok(())
}

