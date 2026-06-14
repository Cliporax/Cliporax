#!/bin/bash
# Install Cliporax CLI to system path
# Usage: ./scripts/install-cli.sh

set -e

echo "📦 Cliporax CLI Installer"
echo "==========================="
echo ""

# Detect OS
OS=$(uname -s)
ARCH=$(uname -m)

echo "🔍 Detected: $OS $ARCH"

# Build CLI
echo ""
echo "🔨 Building Cliporax CLI (release mode)..."
cd src-tauri
cargo build --release --bin cliporax-cli

# Determine binary name
if [ "$OS" = "Linux" ]; then
    BINARY_NAME="cliporax-cli-x86_64-unknown-linux-gnu"
    INSTALL_DIR="/usr/local/bin"
elif [ "$OS" = "Darwin" ]; then
    if [ "$ARCH" = "arm64" ]; then
        BINARY_NAME="cliporax-cli-aarch64-apple-darwin"
    else
        BINARY_NAME="cliporax-cli-x86_64-apple-darwin"
    fi
    INSTALL_DIR="/usr/local/bin"
else
    echo "❌ Unsupported OS: $OS"
    exit 1
fi

# Copy to bin directory for Tauri bundle
echo "📂 Preparing for Tauri bundle..."
mkdir -p bin
cp target/release/cliporax-cli bin/$BINARY_NAME

# Install to system
echo "📦 Installing to $INSTALL_DIR/cliporax..."
sudo cp target/release/cliporax-cli $INSTALL_DIR/cliporax
sudo chmod +x $INSTALL_DIR/cliporax

echo ""
echo "✅ Installation complete!"
echo ""
echo "🎯 You can now use:"
echo "   cliporax get latest          - Get latest clipboard item"
echo "   cliporax get <id>            - Get item by ID"
echo "   cliporax list                - List recent items"
echo "   cliporax search <query>      - Search items"
echo ""
echo "📖 For more info: cliporax --help"
echo ""
echo "💡 Tip: If you used 'npm run tauri build', CLI is already bundled in the installer!"
