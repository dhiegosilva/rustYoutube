# Build script for Windows

Write-Host "Building Rustyoutube for Windows..." -ForegroundColor Green

# Build release binary
cargo build --release

# Create release directory
if (Test-Path release-package) {
    Remove-Item -Recurse -Force release-package
}
New-Item -ItemType Directory -Path release-package | Out-Null

# Copy files
Copy-Item target\release\rustyoutube.exe release-package\
Copy-Item -Recurse locales release-package\
Copy-Item README.md release-package\

# Create ZIP archive
Write-Host "Creating ZIP archive..." -ForegroundColor Green
Compress-Archive -Path release-package\* -DestinationPath rustyoutube-windows-x86_64.zip -Force

Write-Host "Build complete!" -ForegroundColor Green
Write-Host "File created: rustyoutube-windows-x86_64.zip" -ForegroundColor Green

