# Rust YouTube Terminal Client

A terminal-based YouTube client built with Rust that allows you to:
- Authenticate with Google OAuth2
- View your subscription videos
- Play videos using mpv and yt-dlp

## Prerequisites

1. **Rust** - Install from [rustup.rs](https://rustup.rs/)
   - **Windows**: You'll also need a C compiler. Install [MSYS2](https://www.msys2.org/) or use the MSVC toolchain with Visual Studio Build Tools
2. **mpv** - Video player
   - Windows: Download from [mpv.io](https://mpv.io/installation/)
   - Linux: `sudo apt install mpv` (Ubuntu/Debian) or `sudo pacman -S mpv` (Arch)
   - macOS: `brew install mpv`
3. **yt-dlp** - YouTube downloader
   - Windows: Download from [yt-dlp releases](https://github.com/yt-dlp/yt-dlp/releases) or use `pip install yt-dlp`
   - Linux: `sudo apt install yt-dlp` or `pip install yt-dlp`
   - macOS: `brew install yt-dlp` or `pip install yt-dlp`

## Google OAuth Setup (Required)

This app uses **Device Authorization Flow** (like SmartTube) for authentication. You need to create Google OAuth2 credentials:

### Step 1: Create OAuth Credentials

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project or select an existing one
3. Enable the **YouTube Data API v3**:
   - Go to "APIs & Services" → "Library"
   - Search for "YouTube Data API v3"
   - Click "Enable"
4. Configure OAuth Consent Screen:
   - Go to "APIs & Services" → "OAuth consent screen"
   - Choose "External" (unless you have Google Workspace)
   - Fill in app name, support email, developer email
   - Add scopes: `https://www.googleapis.com/auth/youtube.readonly`
   - Add your email as a test user
5. Create OAuth 2.0 Credentials:
   - Go to "APIs & Services" → "Credentials"
   - Click "+ CREATE CREDENTIALS" → "OAuth client ID"
   - **Application type: "TVs and Limited Input devices"** (important!)
   - Give it a name (e.g., "YouTube Terminal Client")
   - Click "Create"
6. Copy your **Client ID** and **Client Secret**

### Step 2: Set Environment Variables

**Windows PowerShell:**
```powershell
$env:GOOGLE_CLIENT_ID="your-client-id-here"
$env:GOOGLE_CLIENT_SECRET="your-client-secret-here"
```

**To make them permanent (Windows):**
```powershell
[System.Environment]::SetEnvironmentVariable("GOOGLE_CLIENT_ID", "your-client-id", "User")
[System.Environment]::SetEnvironmentVariable("GOOGLE_CLIENT_SECRET", "your-client-secret", "User")
```

**Linux/macOS:**
```bash
export GOOGLE_CLIENT_ID="your-client-id-here"
export GOOGLE_CLIENT_SECRET="your-client-secret-here"
```

**To make them permanent (Linux/macOS), add to `~/.bashrc` or `~/.zshrc`:**
```bash
export GOOGLE_CLIENT_ID="your-client-id-here"
export GOOGLE_CLIENT_SECRET="your-client-secret-here"
```

## Building

```bash
cargo build --release
```

## Running

```bash
cargo run --release
```

Or run the binary directly:
```bash
./target/release/rustyoutube
```

## Usage

### First Run - Authentication

1. Run the program: `cargo run --release`
2. The program will display a code and URL (e.g., `https://www.google.com/device`)
3. **On any device** (phone, tablet, another computer), visit that URL
4. Enter the code shown in the terminal
5. Grant permissions to access your YouTube data
6. The program will automatically detect when you've authorized it
7. Your token is saved for future runs

### Navigation

**Main Menu:**
- `s` - View Subscriptions
- `p` - View Playlists  
- `c` - Browse Channel by URL
- `q` - Quit

**In any list view:**
- `↑` / `↓` or `j` / `k` - Navigate
- `Enter` / `Space` - Select/Play
- `r` - Refresh
- `Esc` or `m` - Back to menu
- `b` - Back (in video lists)
- `q` - Quit

**Video Playback**: Videos will open in mpv player. Make sure mpv is installed and in your PATH.

## Features

- ✅ **SmartTube-style Device Authorization Flow** - No browser popup, enter code on any device
- ✅ **View Subscriptions** - Browse all your subscribed channels
- ✅ **View Playlists** - Access all your YouTube playlists
- ✅ **Browse Channels** - Enter any channel URL to view videos
- ✅ **Terminal UI** - Beautiful TUI with ratatui
- ✅ **Video Playback** - Play videos using mpv + yt-dlp
- ✅ **Automatic Token Refresh** - Tokens refresh automatically
- ✅ **Auto-install Dependencies** - mpv and yt-dlp download automatically

## Configuration

The app stores your authentication token in:
- **Windows**: `%APPDATA%\rustyoutube\token.json`
- **Linux/macOS**: `~/.config/rustyoutube/token.json`

## Troubleshooting

- **"mpv not found"**: Make sure mpv is installed and accessible from your PATH
- **"yt-dlp not found"**: Install yt-dlp and ensure it's in your PATH
- **Authentication errors**: Check your Google OAuth credentials and ensure YouTube Data API v3 is enabled
- **No videos showing**: Make sure you have active subscriptions on YouTube

## License

MIT

