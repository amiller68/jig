#!/bin/bash
# jig installer
# Downloads and installs jig from GitHub releases

set -e

REPO="amiller68/jig"
BINARY="jig"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

info() { echo -e "${CYAN}→${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
error() { echo -e "${RED}error:${NC} $1" >&2; exit 1; }

# Detect platform
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Darwin) os="darwin" ;;
        Linux) os="linux" ;;
        *) error "Unsupported OS: $(uname -s)" ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64) arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *) error "Unsupported architecture: $(uname -m)" ;;
    esac

    echo "${arch}-${os}"
}

# Get latest version from GitHub
get_latest_version() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/'
    else
        error "Either curl or wget is required"
    fi
}

# Download file
download() {
    local url="$1"
    local output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$output" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$output" "$url"
    else
        error "Either curl or wget is required"
    fi
}

main() {
    info "Installing jig..."
    echo

    # Detect platform
    local platform
    platform=$(detect_platform)
    info "Detected platform: $platform"

    # Get latest version
    info "Fetching latest version..."
    local version
    version=$(get_latest_version)
    if [ -z "$version" ]; then
        error "Failed to get latest version"
    fi
    info "Latest version: $version"

    # Construct download URL
    local archive="${BINARY}-${version}-${platform}.tar.gz"
    local url="https://github.com/${REPO}/releases/download/${version}/${archive}"

    # Create temp directory
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" EXIT

    # Download
    info "Downloading ${archive}..."
    if ! download "$url" "$tmpdir/$archive"; then
        error "Download failed. Check if the release exists for your platform."
    fi

    # Extract
    info "Extracting..."
    tar -xzf "$tmpdir/$archive" -C "$tmpdir"

    # Find binary (it's in a subdirectory)
    local binary_path
    binary_path=$(find "$tmpdir" -name "$BINARY" -type f | head -1)
    if [ -z "$binary_path" ]; then
        error "Binary not found in archive"
    fi

    # Install
    mkdir -p "$INSTALL_DIR"
    mv "$binary_path" "$INSTALL_DIR/$BINARY"
    chmod +x "$INSTALL_DIR/$BINARY"
    success "Installed $BINARY to $INSTALL_DIR/$BINARY"

    echo
    success "Installation complete!"
    echo

    # Check if INSTALL_DIR is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo -e "${CYAN}Note:${NC} $INSTALL_DIR is not in your PATH."
        echo "Add it to your shell profile:"
        echo
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        echo
    fi

    # Shell integration instructions
    echo "To enable shell integration (cd into worktrees), add to your shell profile:"
    echo
    echo "  # For bash (~/.bashrc)"
    echo "  eval \"\$(jig shell-init bash)\""
    echo
    echo "  # For zsh (~/.zshrc)"
    echo "  eval \"\$(jig shell-init zsh)\""
    echo

    # Verify installation
    if command -v jig >/dev/null 2>&1; then
        echo "Run 'jig --help' to get started."
    else
        echo "Run '$INSTALL_DIR/jig --help' to get started."
    fi
}

main "$@"
