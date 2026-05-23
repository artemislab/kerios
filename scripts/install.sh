#!/usr/bin/env sh
# Install or upgrade `kerios` by downloading the right release artifact
# for the current OS/arch from github.com/artemislab/kerios.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/artemislab/kerios/main/scripts/install.sh | sh
#
# Environment overrides:
#   KERIOS_VERSION=v0.1.0   pin to a specific tag (default: latest release)
#   KERIOS_BIN_DIR=~/.local/bin   where to drop the binary (default below)

set -eu

REPO="artemislab/kerios"
VERSION="${KERIOS_VERSION:-}"
BIN_DIR="${KERIOS_BIN_DIR:-}"

# Pick default install dir
if [ -z "$BIN_DIR" ]; then
    if [ "$(id -u)" = "0" ]; then
        BIN_DIR="/usr/local/bin"
    else
        BIN_DIR="$HOME/.local/bin"
    fi
fi

# Detect OS and arch into the Rust target triple the release workflow ships.
os=$(uname -s)
arch=$(uname -m)
case "$os" in
    Darwin)
        case "$arch" in
            arm64 | aarch64) target="aarch64-apple-darwin" ;;
            x86_64)          target="x86_64-apple-darwin" ;;
            *) echo "unsupported macOS arch: $arch" >&2; exit 1 ;;
        esac ;;
    Linux)
        case "$arch" in
            aarch64 | arm64) target="aarch64-unknown-linux-gnu" ;;
            x86_64)          target="x86_64-unknown-linux-gnu" ;;
            *) echo "unsupported Linux arch: $arch" >&2; exit 1 ;;
        esac ;;
    *) echo "unsupported OS: $os" >&2; exit 1 ;;
esac

# Resolve the version if the user did not pin one.
if [ -z "$VERSION" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
              | grep -m1 '"tag_name":' \
              | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "could not resolve latest release tag; pass KERIOS_VERSION=vX.Y.Z" >&2
        exit 1
    fi
fi

archive="kerios-${VERSION}-${target}.tar.gz"
url="https://github.com/$REPO/releases/download/${VERSION}/${archive}"

echo "downloading $url"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL "$url" -o "$tmp/$archive"
tar -xzf "$tmp/$archive" -C "$tmp"

mkdir -p "$BIN_DIR"
install_path="$BIN_DIR/kerios"
cp "$tmp/kerios-${VERSION}-${target}/kerios" "$install_path"
chmod +x "$install_path"

echo "installed kerios ${VERSION} → $install_path"
"$install_path" --version

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "note: $BIN_DIR is not in PATH — add it (e.g. export PATH=\"$BIN_DIR:\$PATH\")" ;;
esac
