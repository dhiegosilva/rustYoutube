use anyhow::Result;
use tokio::process::Command as TokioCommand;
use std::path::{Path, PathBuf};
use std::fs;
use tokio::io::AsyncWriteExt;

#[cfg(windows)]
const MPV_CMD: &str = "mpv.exe";
#[cfg(not(windows))]
const MPV_CMD: &str = "mpv";

#[cfg(windows)]
const YTDLP_CMD: &str = "yt-dlp.exe";
#[cfg(not(windows))]
const YTDLP_CMD: &str = "yt-dlp";

pub async fn ensure_dependencies() -> Result<()> {
    ensure_mpv().await?;
    ensure_ytdlp().await?;
    Ok(())
}

pub async fn ensure_mpv() -> Result<()> {
    // Check local mpv first (downloaded from GitHub)
    #[cfg(windows)]
    {
        if check_local_mpv().await {
            println!("✓ mpv is installed (local)");
            // Check for updates (only once per day)
            if should_check_for_updates("mpv").await {
                upgrade_mpv_from_github().await.ok();
                update_last_check_time("mpv").await.ok();
            }
            return Ok(());
        }
    }
    
    // Check system mpv
    if check_command(MPV_CMD).await {
        println!("✓ mpv is installed");
        return Ok(());
    }

    println!("mpv not found. Attempting to install...");
    
    #[cfg(windows)]
    {
        // Try downloading from GitHub releases first
        println!("Downloading mpv from GitHub releases...");
        if let Ok(_) = download_mpv_from_github().await {
            // Check if the downloaded mpv works
            if check_local_mpv().await {
                println!("✓ mpv downloaded and installed successfully from GitHub");
                return Ok(());
            }
        }

        // Fallback to package managers
        // Try winget
        if check_command("winget").await {
            println!("Installing mpv via winget...");
            let status = TokioCommand::new("winget")
                .args(&["install", "--id", "Gyan.mpv", "--silent", "--accept-package-agreements", "--accept-source-agreements"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via winget");
                return Ok(());
            }
        }

        // Try chocolatey
        if check_command("choco").await {
            println!("Installing mpv via chocolatey...");
            let status = TokioCommand::new("choco")
                .args(&["install", "mpv", "-y"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via chocolatey");
                return Ok(());
            }
        }

        // Try scoop
        if check_command("scoop").await {
            println!("Installing mpv via scoop...");
            let status = TokioCommand::new("scoop")
                .args(&["install", "mpv"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via scoop");
                return Ok(());
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try apt (Debian/Ubuntu)
        if check_command("apt").await {
            println!("Installing mpv via apt...");
            let status = TokioCommand::new("sudo")
                .args(&["apt", "install", "-y", "mpv"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via apt");
                return Ok(());
            }
        }

        // Try pacman (Arch)
        if check_command("pacman").await {
            println!("Installing mpv via pacman...");
            let status = TokioCommand::new("sudo")
                .args(&["pacman", "-S", "--noconfirm", "mpv"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via pacman");
                return Ok(());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Try brew
        if check_command("brew").await {
            println!("Installing mpv via brew...");
            let status = TokioCommand::new("brew")
                .args(&["install", "mpv"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ mpv installed successfully via brew");
                return Ok(());
            }
        }
    }

    // Final check
    if check_command(MPV_CMD).await {
        println!("✓ mpv is now available");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to install mpv automatically. Please install it manually:\n\
            Windows: winget install Gyan.mpv\n\
            Linux: sudo apt install mpv (or sudo pacman -S mpv)\n\
            macOS: brew install mpv\n\
            Or visit: https://mpv.io/installation/"
        ))
    }
}

pub async fn ensure_ytdlp() -> Result<()> {
    // Check local yt-dlp first (downloaded from GitHub)
    #[cfg(windows)]
    {
        if check_local_ytdlp().await {
            println!("✓ yt-dlp is installed (local)");
            // Check for updates (only once per day)
            if should_check_for_updates("yt-dlp").await {
                upgrade_ytdlp_from_github().await.ok();
                update_last_check_time("yt-dlp").await.ok();
            }
            return Ok(());
        }
    }
    
    // Check system yt-dlp
    if check_command(YTDLP_CMD).await {
        println!("✓ yt-dlp is installed");
        // Check for updates (only once per day)
        if should_check_for_updates("yt-dlp").await {
            upgrade_ytdlp().await.ok();
            update_last_check_time("yt-dlp").await.ok();
        }
        return Ok(());
    }

    println!("yt-dlp not found. Attempting to install...");
    
    #[cfg(windows)]
    {
        // Try downloading from GitHub releases first
        println!("Downloading yt-dlp from GitHub releases...");
        if let Ok(_) = download_ytdlp_from_github().await {
            // Check if the downloaded yt-dlp works
            if check_local_ytdlp().await {
                println!("✓ yt-dlp downloaded and installed successfully from GitHub");
                return Ok(());
            }
        }
        
        // Try pip first (most reliable)
        if check_command("pip").await || check_command("pip3").await {
            let pip_cmd = if check_command("pip3").await { "pip3" } else { "pip" };
            println!("Installing yt-dlp via pip...");
            let status = TokioCommand::new(pip_cmd)
                .args(&["install", "--upgrade", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via pip");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try winget
        if check_command("winget").await {
            println!("Installing yt-dlp via winget...");
            let status = TokioCommand::new("winget")
                .args(&["install", "--id", "yt-dlp.yt-dlp", "--silent", "--accept-package-agreements", "--accept-source-agreements"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via winget");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try chocolatey
        if check_command("choco").await {
            println!("Installing yt-dlp via chocolatey...");
            let status = TokioCommand::new("choco")
                .args(&["install", "yt-dlp", "-y"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via chocolatey");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try scoop
        if check_command("scoop").await {
            println!("Installing yt-dlp via scoop...");
            let status = TokioCommand::new("scoop")
                .args(&["install", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via scoop");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Try pip first
        if check_command("pip3").await || check_command("pip").await {
            let pip_cmd = if check_command("pip3").await { "pip3" } else { "pip" };
            println!("Installing yt-dlp via pip...");
            let status = TokioCommand::new(pip_cmd)
                .args(&["install", "--user", "--upgrade", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via pip");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try apt
        if check_command("apt").await {
            println!("Installing yt-dlp via apt...");
            let status = TokioCommand::new("sudo")
                .args(&["apt", "install", "-y", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via apt");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try pacman
        if check_command("pacman").await {
            println!("Installing yt-dlp via pacman...");
            let status = TokioCommand::new("sudo")
                .args(&["pacman", "-S", "--noconfirm", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via pacman");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Try pip
        if check_command("pip3").await || check_command("pip").await {
            let pip_cmd = if check_command("pip3").await { "pip3" } else { "pip" };
            println!("Installing yt-dlp via pip...");
            let status = TokioCommand::new(pip_cmd)
                .args(&["install", "--upgrade", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via pip");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }

        // Try brew
        if check_command("brew").await {
            println!("Installing yt-dlp via brew...");
            let status = TokioCommand::new("brew")
                .args(&["install", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp installed successfully via brew");
                if check_command(YTDLP_CMD).await {
                    return Ok(());
                }
            }
        }
    }

    // Final check
    if check_command(YTDLP_CMD).await {
        println!("✓ yt-dlp is now available");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to install yt-dlp automatically. Please install it manually:\n\
            Windows: pip install yt-dlp or winget install yt-dlp.yt-dlp\n\
            Linux: pip3 install --user yt-dlp or sudo apt install yt-dlp\n\
            macOS: pip3 install yt-dlp or brew install yt-dlp\n\
            Or visit: https://github.com/yt-dlp/yt-dlp/releases"
        ))
    }
}

async fn upgrade_ytdlp() -> Result<()> {
    println!("Checking for yt-dlp updates...");
    
    #[cfg(windows)]
    {
        // Try pip upgrade
        if check_command("pip").await || check_command("pip3").await {
            let pip_cmd = if check_command("pip3").await { "pip3" } else { "pip" };
            let status = TokioCommand::new(pip_cmd)
                .args(&["install", "--upgrade", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp upgraded via pip");
                return Ok(());
            }
        }

        // Try winget upgrade
        if check_command("winget").await {
            let _ = TokioCommand::new("winget")
                .args(&["upgrade", "--id", "yt-dlp.yt-dlp", "--silent"])
                .status()
                .await;
        }
    }

    #[cfg(not(windows))]
    {
        // Try pip upgrade
        if check_command("pip3").await || check_command("pip").await {
            let pip_cmd = if check_command("pip3").await { "pip3" } else { "pip" };
            let status = TokioCommand::new(pip_cmd)
                .args(&["install", "--user", "--upgrade", "yt-dlp"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                println!("✓ yt-dlp upgraded via pip");
                return Ok(());
            }
        }
    }

    Ok(())
}

#[cfg(windows)]
async fn download_ytdlp_from_github() -> Result<()> {
    use reqwest::Client;
    use serde_json::Value;
    
    // Get the latest release from GitHub API
    let client = Client::new();
    let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
    
    println!("Fetching latest yt-dlp release...");
    let response = client
        .get(url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch release info"));
    }
    
    let release: Value = response.json().await?;
    let tag_name = release["tag_name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Tag name not found"))?;
    
    let assets = release["assets"].as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid release data"))?;
    
    // Find the Windows executable
    let ytdlp_asset = assets.iter()
        .find(|asset| {
            let name = asset["name"].as_str().unwrap_or("");
            name == "yt-dlp.exe"
        })
        .ok_or_else(|| anyhow::anyhow!("yt-dlp.exe not found in release"))?;
    
    let download_url = ytdlp_asset["browser_download_url"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Download URL not found"))?;
    
    println!("Downloading yt-dlp {}...", tag_name);
    
    // Get the local app data directory
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find app data directory"))?;
    let ytdlp_dir = app_data.join("rustyoutube").join("yt-dlp");
    fs::create_dir_all(&ytdlp_dir)?;
    
    let download_path = ytdlp_dir.join("yt-dlp.exe");
    
    // Check if we already have this version
    if download_path.exists() {
        // Check version of existing file
        if let Ok(output) = TokioCommand::new(&download_path)
            .arg("--version")
            .output()
            .await
        {
            if output.status.success() {
                let current_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                // Compare versions (simplified - just check if tag matches)
                if current_version == tag_name.trim_start_matches('v') {
                    println!("✓ yt-dlp is already up to date (version {})", current_version);
                    return Ok(());
                }
            }
        }
    }
    
    // Download the file
    println!("Downloading from: {}", download_url);
    let response = client
        .get(download_url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to download yt-dlp"));
    }
    
    let total_size = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&download_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    
    use futures_util::StreamExt;
    use std::io::Write;
    while let Some(item) = stream.next().await {
        let chunk = item?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        
        if total_size > 0 {
            let percent = (downloaded * 100) / total_size;
            print!("\rDownloading: {}% ({}/{})", percent, downloaded, total_size);
            let _ = std::io::stdout().flush();
        }
    }
    println!("\nDownload complete!");
    
    // Make it executable (on Unix-like systems, this is a no-op on Windows)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata().await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&download_path, perms).await?;
    }
    
    println!("✓ yt-dlp downloaded to: {}", download_path.display());
    
    Ok(())
}

#[cfg(windows)]
async fn upgrade_ytdlp_from_github() -> Result<()> {
    // Check if we have a local yt-dlp
    if !check_local_ytdlp().await {
        return Ok(()); // No local version, skip upgrade
    }
    
    use reqwest::Client;
    use serde_json::Value;
    
    // Get the latest release from GitHub API
    let client = Client::new();
    let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
    
    let response = client
        .get(url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Ok(()); // Silently fail, don't block startup
    }
    
    let release: Value = response.json().await?;
    let tag_name = release["tag_name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Tag name not found"))?;
    
    // Get current version
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find app data directory"))?;
    let ytdlp_path = app_data.join("rustyoutube").join("yt-dlp").join("yt-dlp.exe");
    
    if let Ok(output) = TokioCommand::new(&ytdlp_path)
        .arg("--version")
        .output()
        .await
    {
        if output.status.success() {
            let current_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let latest_version = tag_name.trim_start_matches('v');
            
            if current_version == latest_version {
                // Already up to date
                return Ok(());
            }
            
            println!("Updating yt-dlp from {} to {}...", current_version, latest_version);
        }
    }
    
    // Download new version
    download_ytdlp_from_github().await
}

#[cfg(windows)]
async fn check_local_ytdlp() -> bool {
    if let Some(ytdlp_path) = get_ytdlp_path().await {
        if let Ok(output) = TokioCommand::new(&ytdlp_path)
            .arg("--version")
            .output()
            .await
        {
            return output.status.success();
        }
    }
    false
}

#[cfg(windows)]
pub async fn get_ytdlp_path() -> Option<PathBuf> {
    let app_data = dirs::data_local_dir()?;
    let ytdlp_path = app_data.join("rustyoutube").join("yt-dlp").join("yt-dlp.exe");
    if ytdlp_path.exists() {
        Some(ytdlp_path)
    } else {
        None
    }
}

#[cfg(not(windows))]
pub async fn get_ytdlp_path() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
async fn download_mpv_from_github() -> Result<()> {
    use reqwest::Client;
    use serde_json::Value;
    
    // Get the latest release from GitHub API
    let client = Client::new();
    let url = "https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest";
    
    println!("Fetching latest mpv release...");
    let response = client
        .get(url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to fetch release info"));
    }
    
    let release: Value = response.json().await?;
    let assets = release["assets"].as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid release data"))?;
    
    // Find the mpv-x86_64-v3 release (best performance for modern CPUs)
    let mpv_asset = assets.iter()
        .find(|asset| {
            let name = asset["name"].as_str().unwrap_or("");
            name.starts_with("mpv-x86_64-v3") && name.ends_with(".7z") && !name.contains("debug")
        })
        .ok_or_else(|| anyhow::anyhow!("mpv-x86_64-v3 release not found"))?;
    
    let download_url = mpv_asset["browser_download_url"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Download URL not found"))?;
    let filename = mpv_asset["name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Filename not found"))?;
    
    println!("Downloading {}...", filename);
    
    // Get the local app data directory
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?;
    let mpv_dir = app_data.join("rustyoutube").join("mpv");
    let download_path = mpv_dir.join(filename);
    
    // Create directory if it doesn't exist
    fs::create_dir_all(&mpv_dir)?;
    
    // Check if mpv.exe already exists and is working
    let mpv_exe_path = mpv_dir.join("mpv.exe");
    if mpv_exe_path.exists() {
        // Check if the existing mpv works
        if let Ok(output) = TokioCommand::new(&mpv_exe_path)
            .arg("--version")
            .output()
            .await
        {
            if output.status.success() {
                println!("✓ mpv already exists and is working");
                return Ok(());
            }
        }
    }
    
    // Check if we already have this archive downloaded
    if download_path.exists() {
        // Try extracting it first
        let extract_dir = mpv_dir.join("extracted");
        fs::create_dir_all(&extract_dir)?;
        
        if check_command("7z").await {
            let status = TokioCommand::new("7z")
                .args(&["x", download_path.to_str().unwrap(), &format!("-o{}", extract_dir.to_str().unwrap()), "-y"])
                .status()
                .await;
            
            if status.is_ok() && status.unwrap().success() {
                if let Ok(exe) = find_mpv_exe(&extract_dir) {
                    let final_mpv_path = mpv_dir.join("mpv.exe");
                    if fs::copy(&exe, &final_mpv_path).is_ok() {
                        println!("✓ mpv extracted from existing archive");
                        return Ok(());
                    }
                }
            }
        }
    }
    
    // Download the file
    let mut response = client
        .get(download_url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("Failed to download mpv"));
    }
    
    let mut file = tokio::fs::File::create(&download_path).await?;
    let mut downloaded = 0u64;
    let total_size = response.content_length().unwrap_or(0);
    
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let percent = (downloaded * 100) / total_size;
            print!("\rDownloading: {}% ({}/{})", percent, downloaded, total_size);
            use std::io::Write;
            std::io::stdout().flush().ok();
        }
    }
    println!("\nDownload complete!");
    
    // Extract the 7z file
    println!("Extracting mpv...");
    let extract_dir = mpv_dir.join("extracted");
    fs::create_dir_all(&extract_dir)?;
    
    // Try to use 7z command if available
    if check_command("7z").await {
        let status = TokioCommand::new("7z")
            .args(&["x", download_path.to_str().unwrap(), &format!("-o{}", extract_dir.to_str().unwrap()), "-y"])
            .status()
            .await?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to extract mpv archive with 7z"));
        }
    } else {
        return Err(anyhow::anyhow!(
            "7z not found. Please install 7-Zip to extract mpv.\n\
            Download from: https://www.7-zip.org/\n\
            Or the mpv download will be skipped and you can install mpv manually."
        ));
    }
    
    // Find mpv.exe in the extracted directory
    let mpv_exe = find_mpv_exe(&extract_dir)?;
    
    // Copy to a simpler location
    let final_mpv_path = mpv_dir.join("mpv.exe");
    fs::copy(&mpv_exe, &final_mpv_path)?;
    
    // Clean up downloaded archive
    fs::remove_file(&download_path).ok();
    
    println!("✓ mpv extracted to: {}", final_mpv_path.display());
    
    Ok(())
}

#[cfg(windows)]
fn find_mpv_exe(dir: &Path) -> Result<PathBuf> {
    // Search for mpv.exe in the directory tree
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_dir() {
            if let Ok(found) = find_mpv_exe(&path) {
                return Ok(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some("mpv.exe") {
            return Ok(path);
        }
    }
    
    Err(anyhow::anyhow!("mpv.exe not found in extracted archive"))
}

#[cfg(windows)]
async fn check_local_mpv() -> bool {
    let app_data = match dirs::data_local_dir() {
        Some(dir) => dir,
        None => return false,
    };
    
    let mpv_exe = app_data.join("rustyoutube").join("mpv").join("mpv.exe");
    
    if mpv_exe.exists() {
        // Test if it works
        if let Ok(output) = TokioCommand::new(&mpv_exe)
            .arg("--version")
            .output()
            .await
        {
            return output.status.success();
        }
    }
    
    false
}

#[cfg(windows)]
pub async fn get_mpv_path() -> Option<PathBuf> {
    let app_data = dirs::data_local_dir()?;
    let mpv_exe = app_data.join("rustyoutube").join("mpv").join("mpv.exe");
    
    if mpv_exe.exists() {
        Some(mpv_exe)
    } else {
        None
    }
}

async fn check_command(cmd: &str) -> bool {
    // Try with --version first
    if let Ok(output) = TokioCommand::new(cmd)
        .arg("--version")
        .output()
        .await
    {
        if output.status.success() {
            return true;
        }
    }
    
    // Some commands don't support --version, try -v
    if let Ok(output) = TokioCommand::new(cmd)
        .arg("-v")
        .output()
        .await
    {
        if output.status.success() {
            return true;
        }
    }
    
    // For package managers and tools, try running with --help or just the command
    if let Ok(output) = TokioCommand::new(cmd)
        .arg("--help")
        .output()
        .await
    {
        if output.status.success() {
            return true;
        }
    }
    
    // Last resort: just try to run the command (for commands like pip, winget, etc.)
    // This will fail for most commands, but some package managers might work
    TokioCommand::new(cmd)
        .output()
        .await
        .is_ok()
}

pub async fn check_mpv() -> bool {
    #[cfg(windows)]
    {
        // Check local mpv first
        if check_local_mpv().await {
            return true;
        }
    }
    check_command(MPV_CMD).await
}

pub async fn check_ytdlp() -> bool {
    // Check local yt-dlp first (Windows)
    #[cfg(windows)]
    {
        if check_local_ytdlp().await {
            return true;
        }
    }
    
    // Check system yt-dlp
    check_command(YTDLP_CMD).await
}

// Helper function to check if we should check for updates (once per day)
async fn should_check_for_updates(tool: &str) -> bool {
    let app_data = match dirs::data_local_dir() {
        Some(dir) => dir,
        None => return false,
    };
    let check_file = app_data.join("rustyoutube").join(format!("{}_last_check.txt", tool));
    
    if let Ok(content) = fs::read_to_string(&check_file) {
        if let Ok(timestamp) = content.trim().parse::<i64>() {
            let now = chrono::Utc::now().timestamp();
            let one_day = 24 * 60 * 60;
            // Only check if it's been more than 24 hours
            return (now - timestamp) > one_day;
        }
    }
    
    // No previous check, should check now
    true
}

// Helper function to update the last check timestamp
async fn update_last_check_time(tool: &str) -> Result<()> {
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find app data directory"))?;
    let check_file = app_data.join("rustyoutube").join(format!("{}_last_check.txt", tool));
    
    let timestamp = chrono::Utc::now().timestamp();
    fs::write(&check_file, timestamp.to_string())?;
    Ok(())
}

#[cfg(windows)]
async fn upgrade_mpv_from_github() -> Result<()> {
    // Check if we have a local mpv
    if !check_local_mpv().await {
        return Ok(()); // No local version, skip upgrade
    }
    
    use reqwest::Client;
    use serde_json::Value;
    
    // Get the latest release from GitHub API
    let client = Client::new();
    let url = "https://api.github.com/repos/zhongfly/mpv-winbuild/releases/latest";
    
    let response = client
        .get(url)
        .header("User-Agent", "rustyoutube")
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Ok(()); // Silently fail, don't block startup
    }
    
    let release: Value = response.json().await?;
    let tag_name = release["tag_name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Tag name not found"))?;
    
    // Get current mpv version
    let app_data = dirs::data_local_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find app data directory"))?;
    let mpv_path = app_data.join("rustyoutube").join("mpv").join("mpv.exe");
    
    if let Ok(output) = TokioCommand::new(&mpv_path)
        .arg("--version")
        .output()
        .await
    {
        if output.status.success() {
            let version_output = String::from_utf8_lossy(&output.stdout);
            // mpv version output format: "mpv 0.x.x Copyright..."
            // Extract version number
            let current_version = version_output
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("");
            
            // Check if the release tag contains a newer version
            // For mpv, we'll compare release dates/tags since version format may vary
            // If tag_name is different from what we have, consider it an update
            // We'll download if the release tag is different (simplified check)
            println!("Current mpv version: {}, Latest release: {}", current_version, tag_name);
            
            // For now, we'll just log that an update is available
            // A more sophisticated check would parse versions, but this works for now
            // We'll download if the tag is different (user can manually check if needed)
            // Actually, let's be conservative and not auto-upgrade mpv unless explicitly needed
            // Just log that an update is available
            println!("Note: A newer mpv release ({}) may be available", tag_name);
            return Ok(());
        }
    }
    
    // If we can't get version, don't force upgrade
    Ok(())
}

