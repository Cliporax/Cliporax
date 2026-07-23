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
echo "📦 Installing to $INSTALL_DIR/cliporax-cli..."
sudo cp target/release/cliporax-cli $INSTALL_DIR/cliporax-cli
sudo chmod +x $INSTALL_DIR/cliporax-cli

# Install shell completions
COMPLETION_TMP_DIR=$(mktemp -d)
trap 'rm -rf "$COMPLETION_TMP_DIR"' EXIT

target/release/cliporax-cli completion bash > "$COMPLETION_TMP_DIR/cliporax"
target/release/cliporax-cli completion zsh > "$COMPLETION_TMP_DIR/_cliporax"

echo "⌨️  Installing Bash and Zsh completions..."
sudo mkdir -p /usr/local/share/bash-completion/completions
sudo mkdir -p /usr/local/share/zsh/site-functions
sudo install -m 644 "$COMPLETION_TMP_DIR/cliporax" /usr/local/share/bash-completion/completions/cliporax
sudo install -m 644 "$COMPLETION_TMP_DIR/cliporax" /usr/local/share/bash-completion/completions/cliporax-cli
sudo install -m 644 "$COMPLETION_TMP_DIR/_cliporax" /usr/local/share/zsh/site-functions/_cliporax

echo ""
echo "✅ Installation complete!"
echo ""
echo "🎯 You can now use:"
echo "   cliporax-cli get latest          - Get latest clipboard item"
echo "   cliporax-cli get <id>            - Get item by ID"
echo "   cliporax-cli list                - List recent items"
echo "   cliporax-cli search <query>      - Search items"
echo ""
echo "📖 For more info: cliporax-cli --help"
echo "⌨️  Restart your shell to enable Tab completion."
echo ""
echo "💡 Tip: If you used 'npm run tauri build', CLI is already bundled in the installer!"
