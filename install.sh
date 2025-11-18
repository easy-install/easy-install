#!/bin/bash
#
# Universal Installation Script Template
#
# This script downloads and installs binary releases from GitHub repositories.
# It supports multiple platforms, compression formats, and proxy services.
#
# Configuration:
#   Set EI_DIR environment variable to specify installation directory
#   Example: export EI_DIR=~/.local/bin
#
#   Set EI_TYPE to specify resource type (default: release)
#   - "release": Download from GitHub releases (default)
#   - "file": Download from GitHub raw files
#   Example: export EI_TYPE=file
#
#   Set EI_REF to specify reference for file type (default: main)
#   Used only when EI_TYPE=file
#   Example: export EI_REF=main
#
#   Set EI_MIN_DISK_SPACE to specify minimum required disk space in MB (default: 0)
#   - Set to 0 to skip disk space check (default)
#   - Set to a positive number to enforce minimum disk space requirement
#   Example: export EI_MIN_DISK_SPACE=200
#
# Usage:
#   bash install.sh [OPTIONS]
#
# Options:
#   --proxy <type>    Specify proxy service (github, gh-proxy, xget, jsdelivr, statically)
#   --target <target> Specify target platform (Rust target triple)
#   --tag <version>   Specify release version (default: latest)
#   --help            Display this help message
#
# Examples:
#   bash install.sh
#   bash install.sh --proxy xget
#   bash install.sh --target aarch64-unknown-linux-musl
#   bash install.sh --proxy jsdelivr --tag v1.0.0
#   EI_DIR=~/.ei bash install.sh
#   EI_MIN_DISK_SPACE=200 bash install.sh
#

set -e

# ============================================================================
# CONFIGURATION - Modify these variables to adapt to your project
# ============================================================================
EI_OWNER="easy-install"
EI_REPO="easy-install"
EI_TAG="latest"
EI_BINARY_NAME="ei"
EI_DIR="~/.ei"  # Installation directory (empty = auto-detect based on permissions)

# Resource type: "release" (GitHub release) or "file" (GitHub raw file)
# Default: "release" - downloads from GitHub releases
EI_TYPE="${EI_TYPE:-release}"

# Reference for file type (branch/tag/commit), used only when EI_TYPE="file"
# Examples: "main", "master", "v1.0.0", "abc123"
EI_REF="${EI_REF:-main}"

# EI_MIN_DISK_SPACE to specify minimum required disk space in MB (default: 0)
EI_MIN_DISK_SPACE=10

# ============================================================================
# DEFAULTS - These can be overridden by command-line arguments
# ============================================================================
PROXY="github"
TARGET=""

# ============================================================================
# OS DETECTION - Global variables set once at script start
# ============================================================================
OS_NAME="$(uname -s)"
IS_WINDOWS=false
IS_DARWIN=false

case "$OS_NAME" in
  MINGW*|MSYS*|CYGWIN*|Win*)
    IS_WINDOWS=true
    ;;
  Darwin)
    IS_DARWIN=true
    ;;
esac

# ============================================================================
# PLATFORM MAPPING (Bash 3.2 compatible - no associative arrays)
# ============================================================================

# Get the binary filename for a given Rust target triple
# Args: target (e.g., "x86_64-unknown-linux-musl")
# Returns: filename or empty string if unsupported
get_platform_filename() {
  local target="$1"
  case "$target" in
    x86_64-unknown-linux-musl)      echo "${EI_BINARY_NAME}-x86_64-unknown-linux-musl.tar.gz" ;;
    x86_64-unknown-linux-gnu)       echo "${EI_BINARY_NAME}-x86_64-unknown-linux-gnu.tar.gz" ;;
    aarch64-unknown-linux-musl)     echo "${EI_BINARY_NAME}-aarch64-unknown-linux-musl.tar.gz" ;;
    aarch64-unknown-linux-gnu)      echo "${EI_BINARY_NAME}-aarch64-unknown-linux-gnu.tar.gz" ;;
    armv7-unknown-linux-musleabihf) echo "${EI_BINARY_NAME}-armv7-unknown-linux-musleabihf.tar.gz" ;;
    armv7-unknown-linux-gnueabihf)  echo "${EI_BINARY_NAME}-armv7-unknown-linux-gnueabihf.tar.gz" ;;
    arm-unknown-linux-musleabihf)   echo "${EI_BINARY_NAME}-arm-unknown-linux-musleabihf.tar.gz" ;;
    arm-unknown-linux-gnueabihf)    echo "${EI_BINARY_NAME}-arm-unknown-linux-gnueabihf.tar.gz" ;;
    i686-unknown-linux-musl)        echo "${EI_BINARY_NAME}-i686-unknown-linux-musl.tar.gz" ;;
    i686-unknown-linux-gnu)         echo "${EI_BINARY_NAME}-i686-unknown-linux-gnu.tar.gz" ;;
    x86_64-apple-darwin)            echo "${EI_BINARY_NAME}-x86_64-apple-darwin.tar.gz" ;;
    aarch64-apple-darwin)           echo "${EI_BINARY_NAME}-aarch64-apple-darwin.tar.gz" ;;
    x86_64-pc-windows-msvc)         echo "${EI_BINARY_NAME}-x86_64-pc-windows-msvc.zip" ;;
    x86_64-pc-windows-gnu)          echo "${EI_BINARY_NAME}-x86_64-pc-windows-gnu.zip" ;;
    i686-pc-windows-msvc)           echo "${EI_BINARY_NAME}-i686-pc-windows-msvc.zip" ;;
    aarch64-pc-windows-msvc)        echo "${EI_BINARY_NAME}-aarch64-pc-windows-msvc.zip" ;;
    aarch64-linux-android)          echo "${EI_BINARY_NAME}-aarch64-linux-android.tar.gz" ;;
    armv7-linux-androideabi)        echo "${EI_BINARY_NAME}-armv7-linux-androideabi.tar.gz" ;;
    i686-linux-android)             echo "${EI_BINARY_NAME}-i686-linux-android.tar.gz" ;;
    x86_64-linux-android)           echo "${EI_BINARY_NAME}-x86_64-linux-android.tar.gz" ;;
    *)                              echo "" ;;
  esac
}

# Detect if system is using musl libc
# Returns: "musl" or "gnu"
detect_libc() {
  # Method 1: Check binary files for "musl" string (filesystem-based detection)
  # This is the most reliable method, similar to Rust's is_musl_from_filesystem
  local binary_files="/bin/sh /usr/bin/ldd /lib/libc.so.6 /lib64/libc.so.6"
  for binary in $binary_files; do
    if [ -f "$binary" ]; then
      # Use strings command if available, otherwise use grep with binary flag
      if command -v strings >/dev/null 2>&1; then
        if strings "$binary" 2>/dev/null | grep -q "musl"; then
          echo "musl"
          return
        fi
      else
        # Fallback: use grep to search for "musl" in binary
        if grep -a "musl" "$binary" >/dev/null 2>&1; then
          echo "musl"
          return
        fi
      fi
    fi
  done

  # Method 2: Check ldd version output (child process method)
  if command -v ldd >/dev/null 2>&1; then
    local ldd_output
    ldd_output="$(ldd --version 2>&1)"
    if echo "$ldd_output" | grep -qi "musl"; then
      echo "musl"
      return
    elif echo "$ldd_output" | grep -qi "GLIBC\|GNU libc"; then
      echo "gnu"
      return
    fi
  fi

  # Method 3: Check for musl library files
  if [ -f /lib/libc.musl-x86_64.so.1 ] || \
     [ -f /lib/ld-musl-x86_64.so.1 ] || \
     [ -f /lib/libc.musl-aarch64.so.1 ] || \
     [ -f /lib/ld-musl-aarch64.so.1 ] || \
     [ -f /lib/libc.musl-armhf.so.1 ] || \
     [ -f /lib/ld-musl-armhf.so.1 ]; then
    echo "musl"
    return
  fi

  # Method 4: Check /etc/os-release for Alpine or other musl-based distros
  if [ -f /etc/os-release ]; then
    if grep -qi "alpine\|void" /etc/os-release; then
      echo "musl"
      return
    fi
  fi

  # Method 5: Try to find musl in common locations
  if [ -d /lib/musl ] || [ -d /usr/lib/musl ]; then
    echo "musl"
    return
  fi

  # Method 6: Check if getconf is available and query libc
  if command -v getconf >/dev/null 2>&1; then
    local libc_version
    libc_version="$(getconf GNU_LIBC_VERSION 2>/dev/null)"
    if [ -n "$libc_version" ]; then
      echo "gnu"
      return
    fi
  fi

  # Method 7: Check dynamic linker
  if [ -f /lib/ld-musl-x86_64.so.1 ] || \
     [ -f /lib/ld-musl-aarch64.so.1 ] || \
     [ -f /lib/ld-musl-armhf.so.1 ]; then
    echo "musl"
    return
  fi

  # Default to gnu for most Linux systems
  echo "gnu"
}

# Detect the current platform and return Rust target triple
# Returns: Rust target triple (e.g., "x86_64-unknown-linux-musl")
detect_platform() {
  local os
  local arch
  local libc

  os="$(uname -s)"
  arch="$(uname -m)"

  # Detect libc type on Linux (musl vs gnu)
  libc="gnu"
  if [ "$os" = "Linux" ]; then
    libc="$(detect_libc)"
  fi

  # Check for Android
  if [ "$os" = "Linux" ] && [ "$(uname -o 2>/dev/null)" = "Android" ]; then
    case "$arch" in
      aarch64|arm64)
        echo "aarch64-linux-android"
        return
        ;;
      armv7*|armv8l)
        echo "armv7-linux-androideabi"
        return
        ;;
      i686|x86)
        echo "i686-linux-android"
        return
        ;;
      x86_64)
        echo "x86_64-linux-android"
        return
        ;;
    esac
  fi

  # Map OS and architecture to Rust target triple
  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64)
          echo "x86_64-unknown-linux-${libc}"
          ;;
        aarch64|arm64)
          # Prefer musl on ARM platforms for better compatibility
          if [ "$libc" = "gnu" ]; then
            echo "aarch64-unknown-linux-musl"
          else
            echo "aarch64-unknown-linux-${libc}"
          fi
          ;;
        armv7*|armv8l)
          # Prefer musl on ARM platforms for better compatibility
          if [ "$libc" = "gnu" ]; then
            echo "armv7-unknown-linux-musleabihf"
          else
            echo "armv7-unknown-linux-${libc}eabihf"
          fi
          ;;
        arm*)
          # Prefer musl on ARM platforms for better compatibility
          if [ "$libc" = "gnu" ]; then
            echo "arm-unknown-linux-musleabihf"
          else
            echo "arm-unknown-linux-${libc}eabihf"
          fi
          ;;
        i686|i386)
          echo "i686-unknown-linux-${libc}"
          ;;
        *)
          echo "x86_64-unknown-linux-${libc}"
          ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64|amd64)
          echo "x86_64-apple-darwin"
          ;;
        arm64|aarch64)
          echo "aarch64-apple-darwin"
          ;;
        *)
          echo "x86_64-apple-darwin"
          ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*|Win*)
      case "$arch" in
        x86_64|amd64)
          echo "x86_64-pc-windows-msvc"
          ;;
        i686|i386)
          echo "i686-pc-windows-msvc"
          ;;
        aarch64|arm64)
          echo "aarch64-pc-windows-msvc"
          ;;
        *)
          echo "x86_64-pc-windows-msvc"
          ;;
      esac
      ;;
    *)
      # Default fallback
      echo "x86_64-unknown-linux-musl"
      ;;
  esac
}

# ============================================================================
# UTILITY FUNCTIONS
# ============================================================================

# Display usage information
show_usage() {
  cat << 'EOF'
Universal Installation Script

Usage:
  bash install.sh [OPTIONS]

Options:
  --proxy <type>    Specify proxy service for downloading
                    Available: github, gh-proxy, xget, jsdelivr, statically
                    Default: github

  --target <target> Specify target platform explicitly (Rust target triple)
                    Examples: x86_64-unknown-linux-musl, aarch64-apple-darwin
                    Default: auto-detect

  --tag <version>   Specify release version
                    Default: latest

  --help            Display this help message

Environment Variables:
  EI_DIR              Installation directory (default: ~/.ei)
  EI_TYPE             Resource type: "release" or "file" (default: release)
  EI_REF              Reference for file type (default: main)
                      Used only when EI_TYPE=file
  EI_MIN_DISK_SPACE   Minimum required disk space in MB (default: 0)
                      Set to 0 to skip disk space check
                      Set to positive number to enforce minimum requirement

Examples:
  bash install.sh
  bash install.sh --proxy xget
  bash install.sh --target aarch64-unknown-linux-musl
  bash install.sh --proxy jsdelivr --tag v1.0.0
  EI_DIR=~/.local/bin bash install.sh
  EI_TYPE=file EI_REF=main bash install.sh
  EI_TYPE=file EI_REF=v1.0.0 bash install.sh --proxy jsdelivr
  EI_MIN_DISK_SPACE=100 bash install.sh

Supported Platforms (Rust target triples):
  x86_64-unknown-linux-musl, x86_64-unknown-linux-gnu
  aarch64-unknown-linux-musl, aarch64-unknown-linux-gnu
  armv7-unknown-linux-musleabihf, armv7-unknown-linux-gnueabihf
  x86_64-apple-darwin, aarch64-apple-darwin
  x86_64-pc-windows-msvc, aarch64-pc-windows-msvc
  aarch64-linux-android, armv7-linux-androideabi

Supported Proxies:
  github      - Direct GitHub access (default)
  gh-proxy     - gh-proxy.com mirror
  xget        - xget.xi-xu.me mirror
  jsdelivr    - cdn.jsdelivr.net CDN
  statically  - cdn.statically.io CDN

EOF
}

# Parse command-line arguments
parse_arguments() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --proxy)
        if [ -z "$2" ]; then
          echo "ERROR: --proxy requires a value"
          show_usage
          exit 1
        fi
        PROXY="$2"
        shift 2
        ;;
      --target)
        if [ -z "$2" ]; then
          echo "ERROR: --target requires a value"
          show_usage
          exit 1
        fi
        TARGET="$2"
        shift 2
        ;;
      --tag)
        if [ -z "$2" ]; then
          echo "ERROR: --tag requires a value"
          show_usage
          exit 1
        fi
        EI_TAG="$2"
        shift 2
        ;;
      --dir)
        if [ -z "$2" ]; then
          echo "ERROR: --dir requires a value"
          show_usage
          exit 1
        fi
        EI_DIR="$2"
        shift 2
        ;;
      --help)
        show_usage
        exit 0
        ;;
      *)
        echo "ERROR: Unknown argument: $1"
        show_usage
        exit 1
        ;;
    esac
  done
}

# Resolve path to absolute path (handles ~, relative paths, etc.)
# Args: path
# Returns: absolute path
resolve_path() {
  sh -c "(cd $1 2>/dev/null && pwd -P) || return 1;"
}

resolve_windows_path() {
  local path="$1"
  local abs_path=$(powershell -c "(Resolve-Path '$path').Path")
  echo $abs_path
}


# Detect compression format from filename
# Args: filename
# Returns: format identifier (tar.gz, tgz, tar.xz, zip, unknown)
detect_compression_format() {
  local filename="$1"
  case "$filename" in
    *.tar.gz) echo "tar.gz" ;;
    *.tgz)    echo "tgz" ;;
    *.tar.xz) echo "tar.xz" ;;
    *.zip)    echo "zip" ;;
    *)        echo "unknown" ;;
  esac
}

# Check if a command exists using multiple methods
# Args: command_name
# Returns: 0 if exists, 1 if not
command_exists() {
  local cmd="$1"

  # Method 1: Use command -v (POSIX compliant)
  if command -v "$cmd" >/dev/null 2>&1; then
    return 0
  fi

  # Method 2: Use which (may not be available on all systems)
  if which "$cmd" >/dev/null 2>&1; then
    return 0
  fi

  # Method 3: Use type (bash builtin)
  if type "$cmd" >/dev/null 2>&1; then
    return 0
  fi

  return 1
}

# Check for required dependencies based on compression format
# Args: format (tar.gz, tgz, tar.xz, zip)
# Exits with error if dependencies are missing
check_dependencies() {
  local format="$1"
  local missing_deps=""

  # Always need curl
  if ! command_exists curl; then
    missing_deps="curl"
  fi

  # Check format-specific tools
  case "$format" in
    tar.gz|tgz)
      if ! command_exists tar; then
        missing_deps="${missing_deps} tar"
      fi
      # if ! command_exists gzip; then
      #   missing_deps="${missing_deps} gzip"
      # fi
      ;;
    tar.xz)
      if ! command_exists tar; then
        missing_deps="${missing_deps} tar"
      fi
      # if ! command_exists xz; then
      #   missing_deps="${missing_deps} xz"
      # fi
      ;;
    zip)
      if ! command_exists unzip; then
        missing_deps="${missing_deps} unzip"
      fi
      ;;
  esac

  if [ -n "$missing_deps" ]; then
    echo "ERROR: Missing required dependencies:$missing_deps"
    echo "Please install the missing tools and try again."
    exit 1
  fi
}

# Get available disk space for a directory
# Args: directory path
# Returns: available space in MB, or empty string on failure
get_available_disk_space() {
  local dir="$1"
  local available_space=""

  if [ "$IS_WINDOWS" = true ]; then
    # Use PowerShell for Windows
    available_space=$(powershell -c "[int]((Get-Item '$dir').PSDrive.Free / 1MB)" 2>/dev/null)
  elif [ "$IS_DARWIN" = true ]; then
    # macOS: df uses 512-byte blocks by default, convert to MB
    # Column 4 is available space in 512-byte blocks
    local abs_path=$(resolve_path $EI_DIR)
    available_space=$(df "$abs_path" 2>/dev/null | awk 'NR==2 {printf "%.0f", $4 / 2048}')
  else
    # Linux/Android: use -k for KB output (more compatible), then convert to MB
    # Column 4 is available space in KB
    local abs_path=$(resolve_path $EI_DIR)
    available_space=$(df -k "$abs_path" 2>/dev/null | awk 'NR==2 {printf "%.0f", $4 / 1024}')
  fi

  echo "$available_space"
}

# Check if there is sufficient disk space for installation
# Args: installation directory, minimum required space in MB
# Exits with error if space is insufficient
check_disk_space() {
  local install_dir="$1"
  local required_space="$2"
  required_space=$((required_space  + 0))

  # Skip check if required space is 0
  if [ "$required_space" -eq 0 ]; then
    return 0
  fi

  # Get available disk space
  local available_space
  available_space=$(get_available_disk_space "$install_dir")
  available_space=$((available_space  + 0))

  # If we couldn't get disk space, warn but continue
  if [ -z "$available_space" ]; then
    echo "WARNING: Unable to check disk space, continuing installation" >&2
    return 0
  fi

  # Check if space is sufficient
  if [ "$available_space" -lt "$required_space" ]; then
    local shortage=$((required_space - available_space))
    echo ""
    echo "ERROR: Insufficient disk space for installation" >&2
    echo "  Installation directory: $install_dir" >&2
    echo "  Available space: ${available_space} MB" >&2
    echo "  Required space: ${required_space} MB" >&2
    echo "" >&2
    echo "Please free up at least ${shortage} MB of disk space or choose a different installation directory using:" >&2
    echo "  EI_DIR=/other/path bash install.sh" >&2
    exit 1
  fi
}

# ============================================================================
# DOWNLOAD FUNCTIONS
# ============================================================================

# Generate download URL based on resource type and proxy type
# Args: proxy, owner, repo, tag_or_ref, filename, resource_type
# Returns: download URL
generate_download_url() {
  local proxy="$1"
  local owner="$2"
  local repo="$3"
  local tag_or_ref="$4"
  local filename="$5"
  local resource_type="${6:-release}"  # Default to "release" if not specified

  local github_host="github.com"

  local url=""

  # Generate URL based on resource type
  if [ "$resource_type" = "file" ]; then
    # GitHub raw file format
    case "$proxy" in
      github)
        url="https://${github_host}/${owner}/${repo}/raw/${tag_or_ref}/${filename}"
        ;;
      xget)
        url="https://xget.xi-xu.me/gh/${owner}/${repo}/raw/${tag_or_ref}/${filename}"
        ;;
      gh-proxy)
        url="https://gh-proxy.com/https://${github_host}/${owner}/${repo}/raw/${tag_or_ref}/${filename}"
        ;;
      jsdelivr)
        url="https://cdn.jsdelivr.net/gh/${owner}/${repo}@${tag_or_ref}/${filename}"
        ;;
      statically)
        url="https://cdn.statically.io/gh/${owner}/${repo}/${tag_or_ref}/${filename}"
        ;;
      *)
        echo "ERROR: Unknown proxy type: $proxy" >&2
        echo "Supported proxies: github, gh-proxy, xget, jsdelivr, statically" >&2
        exit 1
        ;;
    esac
  elif [ "$resource_type" = "release" ]; then
    # GitHub release format
    case "$proxy" in
      github)
        if [ "$tag_or_ref" = "latest" ]; then
          url="https://${github_host}/${owner}/${repo}/releases/latest/download/${filename}"
        else
          url="https://${github_host}/${owner}/${repo}/releases/download/${tag_or_ref}/${filename}"
        fi
        ;;
      xget)
        if [ "$tag_or_ref" = "latest" ]; then
          url="https://xget.xi-xu.me/gh/${owner}/${repo}/releases/latest/download/${filename}"
        else
          url="https://xget.xi-xu.me/gh/${owner}/${repo}/releases/download/${tag_or_ref}/${filename}"
        fi
        ;;
      gh-proxy)
        if [ "$tag_or_ref" = "latest" ]; then
          url="https://gh-proxy.com/https://${github_host}/${owner}/${repo}/releases/latest/download/${filename}"
        else
          url="https://gh-proxy.com/https://${github_host}/${owner}/${repo}/releases/download/${tag_or_ref}/${filename}"
        fi
        ;;
      jsdelivr)
        # jsdelivr doesn't support release assets from /releases/download/
        echo "ERROR: jsdelivr proxy does not support GitHub release assets" >&2
        echo "Please use a different proxy (github, gh-proxy, xget) or use EI_TYPE=file" >&2
        exit 1
        ;;
      statically)
        # statically doesn't support release assets from /releases/download/
        echo "ERROR: statically proxy does not support GitHub release assets" >&2
        echo "Please use a different proxy (github, gh-proxy, xget) or use EI_TYPE=file" >&2
        exit 1
        ;;
      *)
        echo "ERROR: Unknown proxy type: $proxy" >&2
        echo "Supported proxies: github, gh-proxy, xget, jsdelivr, statically" >&2
        exit 1
        ;;
    esac
  else
    echo "ERROR: Unknown resource type: $resource_type" >&2
    echo "Supported types: release, file" >&2
    exit 1
  fi

  echo "$url"
}

# Download file from URL
# Args: url, output_path, proxy_name
# Returns: 0 on success, 1 on failure
download_file() {
  local url="$1"
  local output="$2"
  local proxy_name="$3"

  echo "Downloading from $proxy_name..."
  echo "URL: $url"
  if curl --progress-bar --fail --max-time 300 -L "$url" -o "$output"; then
    return 0
  else
    return 1
  fi
}

# ============================================================================
# EXTRACTION FUNCTIONS
# ============================================================================

# Extract archive based on format
# Args: archive_path, destination_dir, format
extract_archive() {
  local archive="$1"
  local dest_dir="$2"
  local format="$3"

  case "$format" in
    tar.gz|tgz)
      tar -xzf "$archive" -C "$dest_dir"
      ;;
    tar.xz)
      tar -xJf "$archive" -C "$dest_dir"
      ;;
    zip)
      unzip -q "$archive" -d "$dest_dir"
      ;;
    *)
      echo "ERROR: Unsupported compression format: $format"
      exit 1
      ;;
  esac
}

# ============================================================================
# CLEANUP FUNCTIONS
# ============================================================================

# Safely cleanup temporary files
# Args: file_path1 [file_path2 ...]
# Only removes files, not directories, to prevent accidental data loss
cleanup_temp_files() {
  local file_path
  for file_path in "$@"; do
    if [ -f "$file_path" ]; then
      echo "Cleaning up temporary file: $file_path"
      rm -f "$file_path"
    fi
  done
}

# ============================================================================
# INSTALLATION FUNCTIONS
# ============================================================================

# Setup installation directory based on OS and permissions
setup_install_dir() {
  if [ "$IS_WINDOWS" = true ]; then
    powershell -c "New-Item -Path '$EI_DIR' -ItemType Directory -Force | Out-Null"
  else
    sh -c "mkdir -p $EI_DIR"
  fi
}

# Update PATH for Fish shell
# Args: install_dir
update_path_fish() {
  local install_dir="$1"
  local fish_config="$HOME/.config/fish/config.fish"

  # Check if already in PATH
  if echo "$PATH" | grep -q "$install_dir"; then
    echo "Installation directory already in PATH"
    return 0
  fi

  # Create Fish config directory if it doesn't exist
  if [ ! -d "$HOME/.config/fish" ]; then
    mkdir -p "$HOME/.config/fish"
  fi

  # Check if already in Fish config file
  if [ -f "$fish_config" ] && grep -Fq "$install_dir" "$fish_config" 2>/dev/null; then
    echo "Installation directory already configured in $fish_config"
    echo "Please restart your shell or run: source $fish_config"
    return 0
  fi

  # Add to Fish config file using Fish syntax
  if [ -w "$fish_config" ] || [ ! -f "$fish_config" ]; then
    echo "" >> "$fish_config"
    echo "# Added by ${EI_BINARY_NAME} installer" >> "$fish_config"
    echo "fish_add_path $install_dir" >> "$fish_config"
    echo "Added $install_dir to $fish_config"
    echo "Please restart your shell or run: source $fish_config"
  else
    echo "WARNING: Cannot write to $fish_config"
    echo "Please manually add the following to your Fish config:"
    echo "  fish_add_path $install_dir"
  fi
}

# Update PATH for Unix-like systems
# Args: install_dir
update_path_unix() {
  local install_dir="$1"
  local profile_file
  local current_shell

  # Detect current shell
  current_shell="$(basename "$SHELL" 2>/dev/null)"

  # Handle Fish shell separately
  if [ "$current_shell" = "fish" ]; then
    update_path_fish "$install_dir"
    return 0
  fi

  # Determine profile file for other shells
  if [ -n "$BASH_VERSION" ]; then
    profile_file="$HOME/.bashrc"
  elif [ -n "$ZSH_VERSION" ]; then
    profile_file="$HOME/.zshrc"
  else
    profile_file="$HOME/.profile"
  fi

  # Check if already in PATH
  if echo "$PATH" | grep -q "$install_dir"; then
    echo "Installation directory already in PATH"
    return 0
  fi

  # Check if already in profile file
  if [ -f "$profile_file" ] && grep -Fq "$install_dir" "$profile_file" 2>/dev/null; then
    echo "Installation directory already configured in $profile_file"
    echo "Please restart your shell or run: source $profile_file"
    return 0
  fi

  # Add to profile file
  if [ -w "$profile_file" ] || [ ! -f "$profile_file" ]; then
    echo "" >> "$profile_file"
    echo "# Added by ${EI_BINARY_NAME} installer" >> "$profile_file"
    echo "export PATH=\"\$PATH:$install_dir\"" >> "$profile_file"
    echo "Added $install_dir to $profile_file"
    echo "Please restart your shell or run: source $profile_file"
  else
    echo "WARNING: Cannot write to $profile_file"
    echo "Please manually add the following to your shell profile:"
    echo "  export PATH=\"\$PATH:$install_dir\""
  fi
}

# Update PATH for Windows systems
# Args: install_dir, path_mode
update_path_windows() {
  local install_dir="$1"
  local cmd="[bool]([System.Security.Principal.WindowsPrincipal][System.Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([System.Security.Principal.WindowsBuiltInRole]::Administrator)";
  local is_admin=$(powershell -c $cmd)
  local mode="User"
  if [ "$is_admin" = "True" ]; then
    mode="Machine"
  else
    mode="User"
  fi
  # Convert MSYS/Git Bash path to Windows path
  local windows_path=$(resolve_windows_path $install_dir)

  # Normalize path separators (use forward slashes for consistency)
  windows_path="$(echo "$windows_path" | sed 's|\\|/|g')"

  echo "Windows path: $windows_path"

  # Check if path already exists
  local has_path
  has_path=$(powershell -c "\$currentPath=[Environment]::GetEnvironmentVariable('Path', '$mode'); [bool](\$currentPath -split ';' | Where-Object { \$_.Replace('\\', '/') -eq '$windows_path' })" 2>/dev/null)

  if [ "$has_path" = "True" ]; then
    echo "Installation directory already in PATH"
    return 0
  fi

  # Add to PATH (PowerShell will handle the path format)
  powershell -c "\$currentPath=[Environment]::GetEnvironmentVariable('Path', '$mode'); \$newPath=\"\$currentPath;$windows_path\"; [Environment]::SetEnvironmentVariable('Path', \$newPath, '$mode')" 2>/dev/null
  echo "Added $windows_path to system PATH ($mode level)"
  echo "Please restart your terminal for PATH changes to take effect"
}

# Add installation directory to GitHub Actions PATH
add_to_github_path() {
  local install_dir="$1"

  if [ "$GITHUB_ACTIONS" = "true" ]; then
    echo "$install_dir" >> "$GITHUB_PATH"
    echo "Added $install_dir to GITHUB_PATH"
  fi
}

# ============================================================================
# MAIN INSTALLATION FLOW
# ============================================================================

main() {
  # Parse command-line arguments
  parse_arguments "$@"

  # Detect or use provided target platform
  if [ -z "$TARGET" ]; then
    TARGET="$(detect_platform)"
    echo "Detected platform: $TARGET"
  else
    echo "Using specified platform: $TARGET"
  fi

  # Get filename for target platform
  FILENAME="$(get_platform_filename "$TARGET")"

  if [ -z "$FILENAME" ]; then
    echo "ERROR: Unsupported platform: $TARGET"
    echo ""
    echo "Supported platforms (Rust target triples):"
    echo "  x86_64-unknown-linux-musl, x86_64-unknown-linux-gnu"
    echo "  aarch64-unknown-linux-musl, aarch64-unknown-linux-gnu"
    echo "  armv7-unknown-linux-musleabihf, armv7-unknown-linux-gnueabihf"
    echo "  x86_64-apple-darwin, aarch64-apple-darwin"
    echo "  x86_64-pc-windows-msvc, aarch64-pc-windows-msvc"
    echo "  aarch64-linux-android, armv7-linux-androideabi"
    exit 1
  fi

  echo "Binary filename: $FILENAME"

  # Detect compression format
  FORMAT="$(detect_compression_format "$FILENAME")"

  if [ "$FORMAT" = "unknown" ]; then
    echo "ERROR: Unknown compression format for file: $FILENAME"
    exit 1
  fi

  echo "Compression format: $FORMAT"

  # Check dependencies
  echo "Checking dependencies..."
  check_dependencies "$FORMAT"
  echo "All dependencies satisfied"

  # Setup installation directory
  setup_install_dir
  local abs_path=$(resolve_path $EI_DIR)
  echo "Installation directory: $abs_path" $EI_DIR

  # Check disk space
  if [ "$EI_MIN_DISK_SPACE" -gt 0 ]; then
    echo "Checking disk space..."
    check_disk_space "$EI_DIR" "$EI_MIN_DISK_SPACE"
    echo "Disk space check passed"
  fi

  # Create temporary download directory
  if command_exists mktemp; then
    DOWNLOAD_DIR="$(mktemp -d)"
  else
    DOWNLOAD_DIR="."
  fi

  DOWNLOAD_PATH="$DOWNLOAD_DIR/$FILENAME"

  # Determine tag or reference based on resource type
  if [ "$EI_TYPE" = "file" ]; then
    TAG_OR_REF="$EI_REF"
    echo "Resource type: file (reference: $TAG_OR_REF)"
  else
    TAG_OR_REF="$EI_TAG"
    echo "Resource type: release (tag: $TAG_OR_REF)"
  fi

  # Generate download URL
  DOWNLOAD_URL="$(generate_download_url "$PROXY" "$EI_OWNER" "$EI_REPO" "$TAG_OR_REF" "$FILENAME" "$EI_TYPE")"

  # Download file
  if ! download_file "$DOWNLOAD_URL" "$DOWNLOAD_PATH" "$PROXY"; then
    echo ""
    echo "ERROR: Failed to download from $PROXY"
    exit 1
  fi

  echo "Download successful!"

  # Extract archive
  echo "Extracting archive..."
  extract_archive "$DOWNLOAD_PATH" "$DOWNLOAD_DIR" "$FORMAT"
  echo "Extraction complete"

  BINARY_PATH=""

  # Look for binary in common locations after extraction
  if [ "$IS_WINDOWS" = true ]; then
    # Windows: look for .exe file
    for possible_path in \
      "$DOWNLOAD_DIR/$EI_BINARY_NAME.exe" \
      "$DOWNLOAD_DIR/$EI_BINARY_NAME" \
      "$DOWNLOAD_DIR/bin/$EI_BINARY_NAME.exe" \
      "$DOWNLOAD_DIR/bin/$EI_BINARY_NAME"; do
      if [ -f "$possible_path" ]; then
        BINARY_PATH="$possible_path"
        break
      fi
    done
  else
    # Unix-like: look for binary without extension
    for possible_path in \
      "$DOWNLOAD_DIR/$EI_BINARY_NAME" \
      "$DOWNLOAD_DIR/bin/$EI_BINARY_NAME" \
      "$DOWNLOAD_DIR/${EI_BINARY_NAME}-"*"/$EI_BINARY_NAME"; do
      if [ -f "$possible_path" ]; then
        BINARY_PATH="$possible_path"
        break
      fi
    done
  fi

  # If still not found, try to find any executable file
  if [ -z "$BINARY_PATH" ]; then
    if [ "$IS_WINDOWS" = true ]; then
      BINARY_PATH="$(find "$DOWNLOAD_DIR" -type f -name "*.exe" 2>/dev/null | head -n 1)"
    else
      BINARY_PATH="$(find "$DOWNLOAD_DIR" -type f -executable -name "$EI_BINARY_NAME" 2>/dev/null | head -n 1)"
    fi
  fi

  if [ -z "$BINARY_PATH" ] || [ ! -f "$BINARY_PATH" ]; then
    echo "ERROR: Could not find binary after extraction"
    echo "Contents of download directory:"
    ls -la "$DOWNLOAD_DIR"
    exit 1
  fi

  echo "Found binary at: $BINARY_PATH"

  # Move binary to installation directory
  echo "Installing binary..."

  if [ "$IS_WINDOWS" = true ]; then
    mv "$BINARY_PATH" "$abs_path/${EI_BINARY_NAME}.exe"
    chmod u+x "$abs_path/${EI_BINARY_NAME}.exe"
    echo "Successfully installed to $abs_path/${EI_BINARY_NAME}.exe"
  else
    mv "$BINARY_PATH" "$abs_path/$EI_BINARY_NAME"
    chmod u+x "$abs_path/$EI_BINARY_NAME"
    echo "Successfully installed to $abs_path/$EI_BINARY_NAME"
  fi

  # Update PATH
  echo "Updating PATH..."
  if [ "$IS_WINDOWS" = true ]; then
    update_path_windows "$EI_DIR"
    win_path=$(resolve_windows_path $EI_DIR)
    add_to_github_path "$win_path"
  else
    update_path_unix "$EI_DIR"
    add_to_github_path "$abs_path"
  fi

  cleanup_temp_files "$DOWNLOAD_PATH"
  echo "Cleanup complete"
}

main "$@"
