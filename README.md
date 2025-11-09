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

## Google OAuth Setup (Optional but Recommended)

For better authentication, you should create your own Google OAuth2 credentials:

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project or select an existing one
3. Enable the YouTube Data API v3
4. Create OAuth 2.0 credentials (Desktop app)
5. Set the redirect URI to `http://localhost:8080`
6. Set environment variables:
   ```bash
   export GOOGLE_CLIENT_ID="your-client-id"
   export GOOGLE_CLIENT_SECRET="your-client-secret"
   ```

If you don't set these, the app will use a default OAuth client (may have rate limits).

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

1. **First Run**: The app will open a browser for Google authentication. Complete the OAuth flow.
2. **Navigation**: 
   - `↑` / `↓` or `j` / `k`: Navigate through videos
   - `Enter` / `Space`: Play selected video
   - `r`: Refresh video list
   - `q`: Quit

3. **Video Playback**: Videos will open in mpv player. Make sure mpv is installed and in your PATH.

## Features

- ✅ Google OAuth2 authentication
- ✅ View subscription videos
- ✅ Terminal UI with ratatui
- ✅ Video playback via mpv + yt-dlp
- ✅ Automatic token refresh
- ✅ Sorted by publish date (newest first)

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

