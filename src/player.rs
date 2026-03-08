use crate::deps;
use anyhow::Result;
use std::process::{Command as StdCommand, Stdio};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::mpsc;

// VP9 video + Ogg-family audio (Opus, then Vorbis) only. Merged by yt-dlp and piped to VLC.
// See https://github.com/yt-dlp/yt-dlp#format-selection
const STREAM_FORMAT: &str = "bestvideo[vcodec^=vp9][height<=1080]+bestaudio[acodec^=opus]/bestaudio[acodec^=vorbis]";
// Same for downloads
const FORMAT_SELECTOR: &str = "bestvideo[vcodec^=vp9][height<=1080]+bestaudio[acodec^=opus]/bestaudio[acodec^=vorbis]";

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

// Helper: Capture stderr and collect it for error messages (used by download path)
#[allow(dead_code)]
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

pub async fn play_video(
    video_id: &str,
    log_tx: Option<mpsc::UnboundedSender<String>>,
) -> Result<()> {
    let send_log = |msg: &str| {
        if let Some(ref tx) = log_tx {
            let _ = tx.send(msg.to_string());
        }
    };

    if !deps::check_vlc().await {
        send_log("VLC not found, attempting to install...");
        deps::ensure_vlc().await?;
    }
    if !deps::check_ytdlp().await {
        send_log("yt-dlp not found, attempting to install...");
        deps::ensure_ytdlp().await?;
    }

    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    send_log(&format!("Preparing to play (VP9 + Ogg): {}", video_id));

    let ytdlp_cmd = get_ytdlp_path().await;
    let (vlc_cmd, vlc_args) = deps::get_vlc_play_stdin_invocation().await;
    send_log("Starting yt-dlp (VP9 + Opus/Vorbis) → VLC...");

    let status = tokio::task::spawn_blocking({
        let url = url.clone();
        let ytdlp_cmd = ytdlp_cmd.clone();
        let vlc_cmd = vlc_cmd.clone();
        let vlc_args = vlc_args.clone();
        move || -> Result<std::process::ExitStatus> {
            let mut ytdlp = StdCommand::new(&ytdlp_cmd)
                .arg("-f")
                .arg(STREAM_FORMAT)
                .arg("-o")
                .arg("-")
                .arg("--no-warnings")
                .arg(&url)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
            let ytdlp_stdout = ytdlp
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("yt-dlp stdout not captured"))?;
            let mut vlc = StdCommand::new(&vlc_cmd)
                .args(&vlc_args)
                .stdin(ytdlp_stdout)
                .spawn()?;
            let status = vlc.wait()?;
            let _ = ytdlp.wait();
            Ok(status)
        }
    })
    .await
    .map_err(|e| anyhow::anyhow!("playback task join: {}", e))??;

    if !status.success() {
        let exit_code = status.code();
        send_log(&format!("VLC exited with code: {:?}", exit_code));
        return Err(anyhow::anyhow!(
            "Playback failed (VLC exit code: {:?}). No VP9+Ogg format may be available for this video.",
            exit_code
        ));
    }

    send_log("Playback completed.");
    Ok(())
}

pub async fn download_video(
    video_id: &str,
    log_tx: Option<mpsc::UnboundedSender<String>>,
    handle_storage: Option<Arc<std::sync::Mutex<Option<tokio::process::Child>>>>,
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
        return Err(anyhow::anyhow!(
            "Handle storage required for cancellation support"
        ));
    };

    if !status.success() {
        send_log(&format!(
            "Download failed with exit code: {:?}",
            status.code()
        ));
        return Err(anyhow::anyhow!(
            "Download failed with exit code: {:?}",
            status.code()
        ));
    }
    send_log("Download completed successfully!");
    Ok(())
}
