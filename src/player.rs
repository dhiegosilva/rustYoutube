use crate::deps;
use anyhow::Result;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

pub async fn play_video(video_id: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> Result<()> {
    // Helper function to send log messages
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    // Ensure dependencies are available before playing
    if !deps::check_mpv().await {
        send_log("mpv not found, attempting to install...");
        deps::ensure_mpv().await?;
    }
    if !deps::check_ytdlp().await {
        send_log("yt-dlp not found, attempting to install...");
        deps::ensure_ytdlp().await?;
    }
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    send_log(&format!("Preparing to play video: {}", video_id));
    
    // Use yt-dlp with --exec to launch mpv directly
    // This is more reliable and provides better feedback
    
    // Use local yt-dlp if available
    #[cfg(windows)]
    let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
        local_ytdlp
    } else {
        std::path::PathBuf::from("yt-dlp.exe")
    };
    #[cfg(not(windows))]
    let ytdlp_cmd = std::path::PathBuf::from("yt-dlp");
    
    // Use local mpv if available
    #[cfg(windows)]
    let mpv_cmd = if let Some(local_mpv) = deps::get_mpv_path().await {
        local_mpv.to_string_lossy().to_string()
    } else {
        "mpv.exe".to_string()
    };
    #[cfg(not(windows))]
    let mpv_cmd = "mpv".to_string();
    
    // Format selector: prefer av01, then vp09, then anything else
    let format_selector = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";
    
    // Use --exec to launch mpv directly with progress output
    send_log("Fetching video stream with yt-dlp (preferring av01 > vp09 > other)...");
    let exec_cmd = format!("{} --no-terminal --really-quiet --", mpv_cmd);
    
    let mut ytdlp = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(format_selector)
        .arg("--no-playlist")
        .arg("--progress")
        .arg("--newline")
        .arg("--exec")
        .arg(&exec_cmd)
        .arg(&url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture and print yt-dlp output in real-time
    let log_tx_stdout = log_tx.clone();
    if let Some(stdout) = ytdlp.stdout.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Some(ref tx) = log_tx_stdout {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    
    let log_tx_stderr = log_tx.clone();
    if let Some(stderr) = ytdlp.stderr.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stderr);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Some(ref tx) = log_tx_stderr {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    
    send_log("Starting mpv player...");
    let status = ytdlp.wait().await?;
    
    if !status.success() {
        let exit_code = status.code();
        send_log(&format!("yt-dlp/mpv exited with code: {:?}", exit_code));
        return Err(anyhow::anyhow!("Video playback failed. Exit code: {:?}", exit_code));
    }
    
    send_log("Video playback completed.");
    Ok(())
}

async fn play_video_fallback(url: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> Result<()> {
    // Fallback: Use yt-dlp to get URL and play with mpv
    // This is simpler and more reliable
    
    // Helper function to send log messages
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
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
        
        // Format selector: prefer av01, then vp09, then anything else
        let format_selector = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";
        
        send_log("Fetching video URL with yt-dlp...");
        let output = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(format_selector)
        .arg("--get-url")
        .arg("--no-warnings")
        .arg(url)
        .output()
        .await?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        let stdout_msg = String::from_utf8_lossy(&output.stdout);
        send_log(&format!("yt-dlp failed. stderr: {}", error_msg));
        send_log(&format!("yt-dlp stdout: {}", stdout_msg));
        return Err(anyhow::anyhow!("Failed to get video URL from yt-dlp. Error: {}", error_msg));
    }

    let video_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    
    if video_url.is_empty() {
        let stderr_msg = String::from_utf8_lossy(&output.stderr);
        send_log(&format!("yt-dlp returned empty URL. stderr: {}", stderr_msg));
        return Err(anyhow::anyhow!("Empty video URL from yt-dlp. Error: {}", stderr_msg));
    }
    
    send_log("Got video URL, starting mpv...");

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

pub async fn download_video(video_id: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> Result<()> {
    // Ensure yt-dlp is available
    if !deps::check_ytdlp().await {
        deps::ensure_ytdlp().await?;
    }
    
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    // Use local yt-dlp if available
    #[cfg(windows)]
    let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
        local_ytdlp.to_str().unwrap().to_string()
    } else {
        "yt-dlp.exe".to_string()
    };
    #[cfg(not(windows))]
    let ytdlp_cmd = "yt-dlp";
    
    // Download video with best quality, prefer av01, then vp09, then anything else
    let format_selector = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";
    
    // Helper function to send log messages
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    send_log("Starting download with yt-dlp...");
    let mut download = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(format_selector)
        .arg("--progress")
        .arg("--newline")
        .arg("--output")
        .arg("%(title)s.%(ext)s")
        .arg(&url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    // Capture and print output in real-time
    let log_tx_stdout = log_tx.clone();
    if let Some(stdout) = download.stdout.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Some(ref tx) = log_tx_stdout {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    
    let log_tx_stderr = log_tx.clone();
    if let Some(stderr) = download.stderr.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stderr);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            if let Some(ref tx) = log_tx_stderr {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    let status = download.wait().await?;
    if !status.success() {
        send_log(&format!("Download failed with exit code: {:?}", status.code()));
        return Err(anyhow::anyhow!("Download failed with exit code: {:?}", status.code()));
    }
    send_log("Download completed successfully!");
    Ok(())
}

