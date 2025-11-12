#!/bin/bash
# Build script for Linux - Creates self-contained AppImage

set -e

echo "Building Rustyoutube for Linux (AppImage)..."

# Build release binary
cargo build --release

# Create AppImage
echo "Creating self-contained AppImage..."
mkdir -p AppDir/usr/bin
mkdir -p AppDir/usr/share/applications
mkdir -p AppDir/usr/share/icons/hicolor/256x256/apps
mkdir -p AppDir/usr/share/rustyoutube

# Copy binary
cp target/release/rustyoutube AppDir/usr/bin/rustyoutube
chmod +x AppDir/usr/bin/rustyoutube

# Copy locales (both locations for compatibility)
cp -r locales AppDir/usr/share/rustyoutube/
cp -r locales AppDir/

# Create .desktop file
cat > AppDir/usr/share/applications/rustyoutube.desktop << 'EOF'
[Desktop Entry]
Type=Application
Name=YouTube Terminal Client
Comment=A terminal-based YouTube client built with Rust
Exec=rustyoutube
Icon=rustyoutube
Terminal=true
Categories=Network;Video;
EOF

# Create AppRun script
cat > AppDir/AppRun << 'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "${0}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export XDG_DATA_DIRS="${HERE}/usr/share:${XDG_DATA_DIRS}"
cd "${HERE}"
exec "${HERE}/usr/bin/rustyoutube" "$@"
EOF
chmod +x AppDir/AppRun

# Check if appimagetool is available
if command -v appimagetool &> /dev/null; then
    echo "Using system appimagetool..."
    ARCH=x86_64 appimagetool AppDir rustyoutube-linux-x86_64.AppImage
elif [ -f "appimagetool-x86_64.AppImage" ]; then
    echo "Using local appimagetool..."
    chmod +x appimagetool-x86_64.AppImage
    ARCH=x86_64 ./appimagetool-x86_64.AppImage AppDir rustyoutube-linux-x86_64.AppImage
else
    echo "Downloading appimagetool..."
    wget -q https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x appimagetool-x86_64.AppImage
    ARCH=x86_64 ./appimagetool-x86_64.AppImage AppDir rustyoutube-linux-x86_64.AppImage
fi

# Make AppImage executable
chmod +x rustyoutube-linux-x86_64.AppImage

echo "Build complete!"
echo "Created: rustyoutube-linux-x86_64.AppImage"
echo ""
echo "The AppImage is self-contained and includes all necessary libraries."
echo "To run: chmod +x rustyoutube-linux-x86_64.AppImage && ./rustyoutube-linux-x86_64.AppImage"

