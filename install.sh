#!/usr/bin/env bash
set -e

REPO="vmargb/myoso"
BIN_NAME="myoso"

# detect OS
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)   TARGET="x86_64-unknown-linux-gnu" ;;
  Darwin)  TARGET="x86_64-apple-darwin" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

if [[ "$ARCH" != "x86_64" ]]; then
  echo "Unsupported architecture: $ARCH"
  exit 1
fi

# get latest release
echo "Fetching latest release..."
TAG=$(curl -s https://api.github.com/repos/$REPO/releases/latest | grep tag_name | cut -d '"' -f4)

if [[ -z "$TAG" ]]; then
  echo "Failed to fetch latest version"
  exit 1
fi

ARTIFACT="${BIN_NAME}-${TARGET}.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ARTIFACT"

echo "Downloading $URL..."

TMP_DIR=$(mktemp -d)
cd "$TMP_DIR"

curl -L "$URL" -o "$ARTIFACT"

echo "Extracting..."
tar -xzf "$ARTIFACT"

# install location
INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"

echo "Installing to $INSTALL_DIR..."
# remove old binary to avoid "Text file busy" errors during self-update
rm -f "$INSTALL_DIR/$BIN_NAME" 
mv "$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

# PATH check
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "$INSTALL_DIR is not in your PATH"
  echo "Add this to your shell config (~/.bashrc, ~/.zshrc):"
  echo ""
  echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
else
  echo "Installed successfully!"
fi

echo ""
echo "Run it with: $BIN_NAME"
