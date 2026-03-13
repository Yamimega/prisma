#!/bin/bash
# Prisma installer for Linux and macOS
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/install.sh | bash -s -- --setup
#   PRISMA_INSTALL_DIR=~/.local/bin bash install.sh
set -euo pipefail

REPO="Yamimega/prisma"
INSTALL_DIR="${PRISMA_INSTALL_DIR:-/usr/local/bin}"
BINARY="prisma"
SETUP=false

for arg in "$@"; do
    case "$arg" in
        --setup) SETUP=true ;;
        --help|-h)
            echo "Usage: install.sh [--setup]"
            echo ""
            echo "Options:"
            echo "  --setup    Generate credentials, TLS certificate, and example configs"
            echo ""
            echo "Environment variables:"
            echo "  PRISMA_INSTALL_DIR   Install directory (default: /usr/local/bin)"
            echo "  PRISMA_CONFIG_DIR    Config output directory for --setup (default: current dir)"
            exit 0
            ;;
    esac
done

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
    linux)  OS="linux" ;;
    darwin) OS="darwin" ;;
    freebsd) OS="freebsd" ;;
    *)
        echo "Error: unsupported OS '$OS'. Supported: linux, darwin, freebsd"
        exit 1
        ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64|amd64)   ARCH="amd64" ;;
    aarch64|arm64)   ARCH="arm64" ;;
    armv7l|armhf)    ARCH="armv7" ;;
    *)
        echo "Error: unsupported architecture '$ARCH'. Supported: x86_64, aarch64, armv7l"
        exit 1
        ;;
esac

DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/prisma-${OS}-${ARCH}"

echo "==> Detected platform: ${OS}/${ARCH}"
echo "==> Downloading prisma from ${DOWNLOAD_URL}..."

TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT

if command -v curl &>/dev/null; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMP"
elif command -v wget &>/dev/null; then
    wget -qO "$TMP" "$DOWNLOAD_URL"
else
    echo "Error: neither curl nor wget found. Install one and retry."
    exit 1
fi

chmod +x "$TMP"

echo "==> Installing to ${INSTALL_DIR}/${BINARY}"
if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP" "${INSTALL_DIR}/${BINARY}"
else
    sudo mv "$TMP" "${INSTALL_DIR}/${BINARY}"
fi
trap - EXIT  # cancel cleanup since we moved the file

echo "==> prisma installed successfully"

# Optional setup
if [ "$SETUP" = true ]; then
    CONFIG_DIR="${PRISMA_CONFIG_DIR:-$(pwd)}"
    PRISMA="${INSTALL_DIR}/${BINARY}"

    echo ""
    echo "==> Running initial setup in ${CONFIG_DIR}..."

    echo "    Generating client credentials..."
    "$PRISMA" gen-key > "${CONFIG_DIR}/.prisma-credentials"

    echo "    Generating TLS certificate..."
    "$PRISMA" gen-cert --output "${CONFIG_DIR}" --cn prisma-server

    # Copy example configs if not already present
    if [ ! -f "${CONFIG_DIR}/server.toml" ]; then
        SCRIPT_URL="https://raw.githubusercontent.com/${REPO}/master/server.example.toml"
        if curl -fsSL "$SCRIPT_URL" -o "${CONFIG_DIR}/server.toml" 2>/dev/null; then
            echo "    Created server.toml from example"
        fi
    fi
    if [ ! -f "${CONFIG_DIR}/client.toml" ]; then
        SCRIPT_URL="https://raw.githubusercontent.com/${REPO}/master/client.example.toml"
        if curl -fsSL "$SCRIPT_URL" -o "${CONFIG_DIR}/client.toml" 2>/dev/null; then
            echo "    Created client.toml from example"
        fi
    fi

    echo ""
    echo "Setup complete!"
    echo "  Credentials: ${CONFIG_DIR}/.prisma-credentials"
    echo "  TLS cert:    ${CONFIG_DIR}/prisma-cert.pem"
    echo "  TLS key:     ${CONFIG_DIR}/prisma-key.pem"
    echo ""
    echo "Next steps:"
    echo "  1. Edit server.toml — paste the client ID and auth secret from .prisma-credentials"
    echo "  2. Edit client.toml — set server_addr and paste the same credentials"
    echo "  3. Run: prisma server -c server.toml"
    echo "  4. Run: prisma client -c client.toml"
fi

echo ""
"${INSTALL_DIR}/${BINARY}" --version 2>/dev/null || echo "Run 'prisma --help' to get started."
