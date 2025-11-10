use crate::deps;
use anyhow::Result;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
struct HardwareCapabilities {
    hwdec_available: Vec<String>,
    has_gpu: bool,
    performance_level: PerformanceLevel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PerformanceLevel {
    High,   // Modern GPU, hardware decoding available
    Medium, // Some acceleration available
    Low,    // Software decoding only
}

// Detect hardware capabilities by querying mpv
async fn detect_hardware_capabilities(mpv_cmd: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> HardwareCapabilities {
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    let mut hwdec_available = Vec::new();
    let mut has_gpu = false;
    let mut performance_level = PerformanceLevel::Low;
    
    // Try to detect hardware decoding support
    if let Ok(output) = TokioCommand::new(mpv_cmd)
        .arg("--hwdec=help")
        .output()
        .await
    {
        let output_str = String::from_utf8_lossy(&output.stdout);
        if output_str.contains("auto") || output_str.contains("auto-safe") {
            hwdec_available.push("auto-safe".to_string());
        }
        if output_str.contains("d3d11va") {
            hwdec_available.push("d3d11va".to_string());
            has_gpu = true;
        }
        if output_str.contains("nvdec") {
            hwdec_available.push("nvdec".to_string());
            has_gpu = true;
        }
        if output_str.contains("vaapi") {
            hwdec_available.push("vaapi".to_string());
            has_gpu = true;
        }
        if output_str.contains("videotoolbox") {
            hwdec_available.push("videotoolbox".to_string());
            has_gpu = true;
        }
    }
    
    // Determine performance level
    if has_gpu && !hwdec_available.is_empty() {
        performance_level = PerformanceLevel::High;
    } else if !hwdec_available.is_empty() {
        performance_level = PerformanceLevel::Medium;
    }
    
    send_log(&format!("Detected hardware: GPU={}, HWDec={:?}, Level={:?}", 
        has_gpu, hwdec_available, performance_level));
    
    HardwareCapabilities {
        hwdec_available,
        has_gpu,
        performance_level,
    }
}

// Build optimized mpv arguments based on hardware capabilities
fn build_mpv_args(mpv_cmd: &str, caps: &HardwareCapabilities) -> String {
    let mut args = vec![
        mpv_cmd.to_string(),
        "--no-terminal".to_string(),
        "--really-quiet".to_string(),
    ];
    
    // Hardware acceleration
    if !caps.hwdec_available.is_empty() {
        // Prefer auto-safe, then platform-specific
        let hwdec = if caps.hwdec_available.contains(&"auto-safe".to_string()) {
            "auto-safe"
        } else if caps.hwdec_available.contains(&"d3d11va".to_string()) {
            "d3d11va"
        } else if caps.hwdec_available.contains(&"nvdec".to_string()) {
            "nvdec"
        } else if caps.hwdec_available.contains(&"vaapi".to_string()) {
            "vaapi"
        } else {
            caps.hwdec_available.first().map(|s| s.as_str()).unwrap_or("auto")
        };
        args.push(format!("--hwdec={}", hwdec));
        args.push("--hwdec-codecs=all".to_string());
    }
    
    // Performance settings based on hardware level
    match caps.performance_level {
        PerformanceLevel::High => {
            // High-end: Maximum quality and performance
            args.push("--profile=gpu-hq".to_string());
            args.push("--vo=gpu".to_string());
            args.push("--scale=ewa_lanczossharp".to_string());
            args.push("--cscale=ewa_lanczossharp".to_string());
            args.push("--dscale=ewa_lanczossharp".to_string());
            args.push("--deband=yes".to_string());
            args.push("--dither-depth=auto".to_string());
            args.push("--cache=yes".to_string());
            args.push("--cache-secs=60".to_string());
            args.push("--demuxer-readahead-secs=30".to_string());
            args.push("--stream-buffer-size=2MiB".to_string());
            args.push("--cache-pause=yes".to_string());
            args.push("--vd-lavc-threads=0".to_string());
            args.push("--vd-lavc-fast=yes".to_string());
        },
        PerformanceLevel::Medium => {
            // Medium: Balanced quality and performance
            args.push("--vo=gpu".to_string());
            args.push("--scale=lanczos".to_string());
            args.push("--cscale=lanczos".to_string());
            args.push("--deband=yes".to_string());
            args.push("--cache=yes".to_string());
            args.push("--cache-secs=45".to_string());
            args.push("--demuxer-readahead-secs=25".to_string());
            args.push("--stream-buffer-size=1.5MiB".to_string());
            args.push("--vd-lavc-threads=0".to_string());
        },
        PerformanceLevel::Low => {
            // Low-end: Performance over quality
            args.push("--vo=gpu".to_string());
            args.push("--cache=yes".to_string());
            args.push("--cache-secs=30".to_string());
            args.push("--demuxer-readahead-secs=20".to_string());
            args.push("--stream-buffer-size=1MiB".to_string());
            args.push("--framedrop=vo".to_string());
            args.push("--vd-lavc-threads=0".to_string());
        },
    }
    
    // Common optimizations for all levels
    args.push("--target-prim=auto".to_string());
    args.push("--target-trc=auto".to_string());
    args.push("--".to_string());
    
    args.join(" ")
}

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
    
    // Detect hardware capabilities
    send_log("Detecting hardware capabilities...");
    let caps = detect_hardware_capabilities(&mpv_cmd, log_tx.clone()).await;
    
    // Build optimized mpv command
    let exec_cmd = build_mpv_args(&mpv_cmd, &caps);
    send_log(&format!("Using optimized settings: {:?}", caps.performance_level));
    
    // Format selector: prefer av01, then vp09, then anything else
    let format_selector = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";
    
    // Use --exec to launch mpv directly with progress output
    send_log("Fetching video stream with yt-dlp (preferring av01 > vp09 > other)...");
    
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

