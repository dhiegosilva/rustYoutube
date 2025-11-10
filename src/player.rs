use crate::deps;
use anyhow::Result;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use std::sync::Arc;

// Format selector: prefer av01, then vp09, then anything else
const FORMAT_SELECTOR: &str = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";

#[derive(Debug, Clone)]
struct HardwareCapabilities {
    hwdec_available: Vec<String>,
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
        }
        if output_str.contains("nvdec") {
            hwdec_available.push("nvdec".to_string());
        }
        if output_str.contains("vaapi") {
            hwdec_available.push("vaapi".to_string());
        }
        if output_str.contains("videotoolbox") {
            hwdec_available.push("videotoolbox".to_string());
        }
    }
    
    // Determine performance level based on available hardware decoders
    if hwdec_available.len() >= 2 {
        performance_level = PerformanceLevel::High;
    } else if !hwdec_available.is_empty() {
        performance_level = PerformanceLevel::Medium;
    }
    
    send_log(&format!("Detected hardware: HWDec={:?}, Level={:?}", 
        hwdec_available, performance_level));
    
    HardwareCapabilities {
        hwdec_available,
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
    
    // Use --exec to launch mpv directly with progress output
    send_log("Fetching video stream with yt-dlp (preferring av01 > vp09 > other)...");
    
    let mut ytdlp = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(FORMAT_SELECTOR)
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
    
    // Collect stderr output for better error messages
    let mut stderr_output = Vec::new();
    let stderr_handle = if let Some(stderr) = ytdlp.stderr.take() {
        let log_tx_stderr = log_tx.clone();
        Some(tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut reader = BufReader::new(stderr);
            let mut lines = Vec::new();
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            lines.push(trimmed.to_string());
                            if let Some(ref tx) = log_tx_stderr {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            lines.join("\n").into_bytes()
        }))
    } else {
        None
    };
    
    send_log("Starting mpv player...");
    let status = ytdlp.wait().await?;
    
    // Get stderr output if available
    if let Some(handle) = stderr_handle {
        if let Ok(output) = handle.await {
            stderr_output = output;
        }
    }
    
    if !status.success() {
        let exit_code = status.code();
        let error_msg = String::from_utf8_lossy(&stderr_output);
        
        // Provide more helpful error messages based on exit code
        let user_friendly_error = match exit_code {
            Some(120) => {
                if error_msg.contains("HTTP Error 403") || error_msg.contains("Forbidden") {
                    "Video is private or unavailable. Try a different video."
                } else if error_msg.contains("timeout") || error_msg.contains("Connection") {
                    "Network timeout. Check your internet connection and try again."
                } else if error_msg.contains("format") || error_msg.contains("No video formats") {
                    "Format selection failed. Trying fallback format..."
                } else {
                    "Video playback failed. This might be due to network issues or video unavailability."
                }
            }
            Some(1) => "General error occurred. Check the error messages above.",
            Some(2) => "yt-dlp argument error. This is a bug, please report it.",
            _ => "Unknown error occurred during video playback.",
        };
        
        send_log(&format!("Error: {} (Exit code: {:?})", user_friendly_error, exit_code));
        
        // For exit code 120 (format/network issues), try fallback format
        if exit_code == Some(120) && (error_msg.contains("format") || error_msg.contains("No video formats") || error_msg.is_empty()) {
            send_log("Retrying with fallback format (best available)...");
            return play_video_fallback_format(video_id, log_tx).await;
        }
        
        return Err(anyhow::anyhow!(
            "{}\nExit code: {:?}\nError details: {}",
            user_friendly_error,
            exit_code,
            if error_msg.is_empty() { "No additional error details available" } else { &error_msg }
        ));
    }
    
    send_log("Video playback completed.");
    Ok(())
}

// Fallback function to try with simpler format selection
async fn play_video_fallback_format(video_id: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> Result<()> {
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    #[cfg(windows)]
    let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
        local_ytdlp
    } else {
        std::path::PathBuf::from("yt-dlp.exe")
    };
    #[cfg(not(windows))]
    let ytdlp_cmd = std::path::PathBuf::from("yt-dlp");
    
    #[cfg(windows)]
    let mpv_cmd = if let Some(local_mpv) = deps::get_mpv_path().await {
        local_mpv.to_string_lossy().to_string()
    } else {
        "mpv.exe".to_string()
    };
    #[cfg(not(windows))]
    let mpv_cmd = "mpv".to_string();
    
    // Use simpler format selector as fallback
    let fallback_format = "best[height<=1080]/best";
    send_log(&format!("Trying fallback format: {}", fallback_format));
    
    let mut ytdlp = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(fallback_format)
        .arg("--no-playlist")
        .arg("--progress")
        .arg("--newline")
        .arg("--exec")
        .arg(&format!("{} --no-terminal --really-quiet --", mpv_cmd))
        .arg(&url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture output
    let log_tx_stdout = log_tx.clone();
    if let Some(stdout) = ytdlp.stdout.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
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
                    Ok(0) => break,
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
    
    let status = ytdlp.wait().await?;
    
    if !status.success() {
        let exit_code = status.code();
        // Try final fallback with just 'best'
        if exit_code == Some(120) {
            send_log("Fallback format failed, trying basic 'best' format...");
            return play_video_final_fallback(video_id, log_tx).await;
        }
        send_log(&format!("Fallback also failed with exit code: {:?}", exit_code));
        return Err(anyhow::anyhow!(
            "Video playback failed even with fallback format.\nExit code: {:?}\nThe video might be unavailable or your network connection is having issues.",
            exit_code
        ));
    }
    
    send_log("Video playback completed (using fallback format).");
    Ok(())
}

// Final fallback function using just 'best' format
async fn play_video_final_fallback(video_id: &str, log_tx: Option<mpsc::UnboundedSender<String>>) -> Result<()> {
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    #[cfg(windows)]
    let ytdlp_cmd = if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
        local_ytdlp
    } else {
        std::path::PathBuf::from("yt-dlp.exe")
    };
    #[cfg(not(windows))]
    let ytdlp_cmd = std::path::PathBuf::from("yt-dlp");
    
    #[cfg(windows)]
    let mpv_cmd = if let Some(local_mpv) = deps::get_mpv_path().await {
        local_mpv.to_string_lossy().to_string()
    } else {
        "mpv.exe".to_string()
    };
    #[cfg(not(windows))]
    let mpv_cmd = "mpv".to_string();
    
    // Use yt-dlp default format (most compatible) - don't specify --format at all
    send_log("Trying final fallback: using yt-dlp default format (most compatible)...");
    
    let mut ytdlp = TokioCommand::new(&ytdlp_cmd)
        .arg("--no-playlist")
        .arg("--progress")
        .arg("--newline")
        .arg("--exec")
        .arg(&format!("{} --no-terminal --really-quiet --", mpv_cmd))
        .arg(&url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture output
    let log_tx_stdout = log_tx.clone();
    if let Some(stdout) = ytdlp.stdout.take() {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(stdout);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
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
                    Ok(0) => break,
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
    
    let status = ytdlp.wait().await?;
    
    if !status.success() {
        let exit_code = status.code();
        send_log(&format!("Final fallback also failed with exit code: {:?}", exit_code));
        return Err(anyhow::anyhow!(
            "Video playback failed with all format options.\nExit code: {:?}\nThe video might be unavailable, private, or your network connection is having issues.",
            exit_code
        ));
    }
    
    send_log("Video playback completed (using yt-dlp default format).");
    Ok(())
}


pub async fn download_video(
    video_id: &str, 
    log_tx: Option<mpsc::UnboundedSender<String>>,
    handle_storage: Option<Arc<std::sync::Mutex<Option<tokio::process::Child>>>>
) -> Result<()> {
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
    
    // Helper function to send log messages
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };
    
    send_log("Starting download with yt-dlp...");
    let mut download = TokioCommand::new(&ytdlp_cmd)
        .arg("--format")
        .arg(FORMAT_SELECTOR)
        .arg("--progress")
        .arg("--newline")
        .arg("--output")
        .arg("%(title)s.%(ext)s")
        .arg(&url)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Take stdout and stderr before storing handle
    let stdout = download.stdout.take();
    let stderr = download.stderr.take();
    
    // Store the handle for cancellation
    if let Some(ref handle_storage) = &handle_storage {
        let mut guard = handle_storage.lock().unwrap();
        *guard = Some(download);
    }

    // Capture and print output in real-time
    let log_tx_stdout = log_tx.clone();
    if let Some(stdout) = stdout {
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
    if let Some(stderr) = stderr {
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

    // Wait for download to complete
    let status = if let Some(ref handle_storage) = &handle_storage {
        // Take the child out of the mutex before awaiting
        let child = {
            let mut child_guard = handle_storage.lock().unwrap();
            child_guard.take()
        };
        
        if let Some(mut child_process) = child {
            let result = child_process.wait().await;
            // Clear handle after completion
            {
                let mut child_guard = handle_storage.lock().unwrap();
                *child_guard = None;
            }
            match result {
                Ok(status) => status,
                Err(e) => {
                    send_log(&format!("Error waiting for download: {}", e));
                    return Err(anyhow::anyhow!("Error waiting for download: {}", e));
                }
            }
        } else {
            // Handle was already taken (cancelled)
            send_log("Download was cancelled");
            return Err(anyhow::anyhow!("Download was cancelled"));
        }
    } else {
        return Err(anyhow::anyhow!("Handle storage required for cancellation support"));
    };
    
    if !status.success() {
        send_log(&format!("Download failed with exit code: {:?}", status.code()));
        return Err(anyhow::anyhow!("Download failed with exit code: {:?}", status.code()));
    }
    send_log("Download completed successfully!");
    Ok(())
}

