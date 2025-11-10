use crate::deps;
use anyhow::Result;
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

// Format selector: prefer av01, then vp09, then anything else
const FORMAT_SELECTOR: &str = "bestvideo[vcodec^=av01][height<=1080]+bestaudio/best[vcodec^=av01][height<=1080]/bestvideo[vcodec^=vp09][height<=1080]+bestaudio/best[vcodec^=vp09][height<=1080]/best[height<=1080]";

// Helper: Get mpv command path
async fn get_mpv_cmd() -> String {
    #[cfg(windows)]
    {
        if let Some(local_mpv) = deps::get_mpv_path().await {
            local_mpv.to_string_lossy().to_string()
        } else {
            "mpv.exe".to_string()
        }
    }
    #[cfg(not(windows))]
    {
        "mpv".to_string()
    }
}

// Helper: Get yt-dlp command path
async fn get_ytdlp_path() -> String {
    #[cfg(windows)]
    {
        if let Some(local_ytdlp) = deps::get_ytdlp_path().await {
            local_ytdlp.to_string_lossy().to_string()
        } else {
            "yt-dlp.exe".to_string()
        }
    }
    #[cfg(not(windows))]
    {
        "yt-dlp".to_string()
    }
}

// Helper: Capture output from stdout and send to log channel
fn capture_output(
    stream: Option<tokio::process::ChildStdout>,
    log_tx: Option<mpsc::UnboundedSender<String>>,
) {
    if let Some(stdout) = stream {
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
                            if let Some(ref tx) = log_tx {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}

// Helper: Capture stderr and send to log channel (simple version for downloads)
fn capture_stderr_simple(
    stderr: Option<tokio::process::ChildStderr>,
    log_tx: Option<mpsc::UnboundedSender<String>>,
) {
    if let Some(stderr) = stderr {
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
                            if let Some(ref tx) = log_tx {
                                let _ = tx.send(trimmed.to_string());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}

// Helper: Capture stderr and collect it for error messages
fn capture_stderr(
    stderr: Option<tokio::process::ChildStderr>,
    log_tx: Option<mpsc::UnboundedSender<String>>,
) -> tokio::task::JoinHandle<Vec<u8>> {
    let log_tx_stderr = log_tx.clone();
    tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut reader = BufReader::new(stderr);
            let mut lines = Vec::new();
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
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
        } else {
            Vec::new()
        }
    })
}

// Helper: Build mpv args with yt-dlp configuration
async fn build_mpv_args_with_ytdlp(
    caps: &HardwareCapabilities,
    format_selector: Option<&str>,
) -> Vec<String> {
    let mut args = build_mpv_args(caps);
    
    // Configure mpv to use yt-dlp
    let ytdlp_path = get_ytdlp_path().await;
    args.push(format!("--script-opts=ytdl_hook-ytdl_path={}", ytdlp_path));
    
    // Set format selector if provided
    if let Some(format) = format_selector {
        args.push("--ytdl-format".to_string());
        args.push(format.to_string());
    }
    
    args
}

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
        let decoders = [
            ("auto-safe", "auto-safe"),
            ("auto", "auto-safe"),
            ("d3d11va", "d3d11va"),
            ("nvdec", "nvdec"),
            ("vaapi", "vaapi"),
            ("videotoolbox", "videotoolbox"),
        ];
        
        for (search, decoder) in decoders.iter() {
            if output_str.contains(search) && !hwdec_available.contains(&decoder.to_string()) {
                hwdec_available.push(decoder.to_string());
            }
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
fn build_mpv_args(caps: &HardwareCapabilities) -> Vec<String> {
    let mut args = vec![
        "--no-terminal".to_string(),
        "--really-quiet".to_string(),
    ];
    
    // Hardware acceleration
    if !caps.hwdec_available.is_empty() {
        // Prefer auto-safe, then platform-specific
        let hwdec = ["auto-safe", "d3d11va", "nvdec", "vaapi"]
            .iter()
            .find(|&decoder| caps.hwdec_available.contains(&decoder.to_string()))
            .copied()
            .or_else(|| caps.hwdec_available.first().map(|s| s.as_str()))
            .unwrap_or("auto");
        
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
    
    args
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
    // yt-dlp is still needed for mpv's built-in support
    if !deps::check_ytdlp().await {
        send_log("yt-dlp not found, attempting to install...");
        deps::ensure_ytdlp().await?;
    }
    
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    
    send_log(&format!("Preparing to play video: {}", video_id));
    
    let mpv_cmd = get_mpv_cmd().await;
    
    // Detect hardware capabilities
    send_log("Detecting hardware capabilities...");
    let caps = detect_hardware_capabilities(&mpv_cmd, log_tx.clone()).await;
    
    send_log(&format!("Using optimized settings: {:?}", caps.performance_level));
    
    // Build mpv arguments with yt-dlp config and AV01 format preference
    let mut mpv_args = build_mpv_args_with_ytdlp(&caps, Some(FORMAT_SELECTOR)).await;
    
    // Add the YouTube URL
    mpv_args.push(url);
    
    send_log("Streaming video with mpv (preferring av01 > vp09 > other)...");
    
    let mut mpv = TokioCommand::new(&mpv_cmd)
        .args(&mpv_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture output streams
    capture_output(mpv.stdout.take(), log_tx.clone());
    let stderr_handle = capture_stderr(mpv.stderr.take(), log_tx.clone());
    
    send_log("Starting mpv player...");
    let status = mpv.wait().await?;
    
    // Get stderr output if available
    let stderr_output = stderr_handle.await.unwrap_or_default();
    
    if !status.success() {
        let exit_code = status.code();
        let error_msg = String::from_utf8_lossy(&stderr_output);
        
        // Provide more helpful error messages based on exit code
        let user_friendly_error = match exit_code {
            Some(1) | Some(2) => {
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
            _ => "Unknown error occurred during video playback.",
        };
        
        send_log(&format!("Error: {} (Exit code: {:?})", user_friendly_error, exit_code));
        
        // Try fallback format if format selection failed
        if error_msg.contains("format") || error_msg.contains("No video formats") || error_msg.is_empty() {
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
    let mpv_cmd = get_mpv_cmd().await;
    
    // Use simpler format selector as fallback
    let fallback_format = "best[height<=1080]/best";
    send_log(&format!("Trying fallback format: {}", fallback_format));
    
    // Get hardware caps for basic args
    let caps = detect_hardware_capabilities(&mpv_cmd, log_tx.clone()).await;
    let mut mpv_args = build_mpv_args_with_ytdlp(&caps, Some(fallback_format)).await;
    mpv_args.push(url);
    
    let mut mpv = TokioCommand::new(&mpv_cmd)
        .args(&mpv_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture output streams
    capture_output(mpv.stdout.take(), log_tx.clone());
    capture_stderr(mpv.stderr.take(), log_tx.clone());
    
    let status = mpv.wait().await?;
    
    if !status.success() {
        // Try final fallback with just 'best'
        send_log("Fallback format failed, trying basic 'best' format...");
        return play_video_final_fallback(video_id, log_tx).await;
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
    let mpv_cmd = get_mpv_cmd().await;
    
    // Use mpv default format (most compatible) - don't specify --ytdl-format
    send_log("Trying final fallback: using mpv default format (most compatible)...");
    
    // Get hardware caps for basic args
    let caps = detect_hardware_capabilities(&mpv_cmd, log_tx.clone()).await;
    let mut mpv_args = build_mpv_args_with_ytdlp(&caps, None).await;
    mpv_args.push(url);
    
    let mut mpv = TokioCommand::new(&mpv_cmd)
        .args(&mpv_args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    // Capture output streams
    capture_output(mpv.stdout.take(), log_tx.clone());
    capture_stderr(mpv.stderr.take(), log_tx.clone());
    
    let status = mpv.wait().await?;
    
    if !status.success() {
        let exit_code = status.code();
        send_log(&format!("Final fallback also failed with exit code: {:?}", exit_code));
        return Err(anyhow::anyhow!(
            "Video playback failed with all format options.\nExit code: {:?}\nThe video might be unavailable, private, or your network connection is having issues.",
            exit_code
        ));
    }
    
    send_log("Video playback completed (using mpv default format).");
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
    capture_output(stdout, log_tx.clone());
    capture_stderr_simple(stderr, log_tx.clone());

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

