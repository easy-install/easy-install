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

# ============================================================================
# DEFAULTS - These can be overridden by command-line arguments
# ============================================================================
PROXY="github"
TARGET=""

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

Examples:
  bash install.sh
  bash install.sh --proxy xget
  bash install.sh --target aarch64-unknown-linux-musl
  bash install.sh --proxy jsdelivr --tag v1.0.0

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
  local path="$1"
  local abs_path=$(bash -c "realpath $path")
  echo $abs_path
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
      if ! command_exists gzip; then
        missing_deps="${missing_deps} gzip"
      fi
      ;;
    tar.xz)
      if ! command_exists tar; then
        missing_deps="${missing_deps} tar"
      fi
      if ! command_exists xz; then
        missing_deps="${missing_deps} xz"
      fi
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

# ============================================================================
# DOWNLOAD FUNCTIONS
# ============================================================================

# Generate download URL based on proxy type
# Args: proxy, owner, repo, tag, filename
# Returns: download URL
generate_download_url() {
  local proxy="$1"
  local owner="$2"
  local repo="$3"
  local tag="$4"
  local filename="$5"
  local github_url

  # Build base GitHub URL
  if [ "$tag" = "latest" ]; then
    github_url="https://github.com/${owner}/${repo}/releases/latest/download/${filename}"
  else
    github_url="https://github.com/${owner}/${repo}/releases/download/${tag}/${filename}"
  fi

  # Apply proxy transformation
  case "$proxy" in
    github)
      echo "$github_url"
      ;;
    gh-proxy)
      echo "https://gh-proxy.com/${github_url}"
      ;;
    xget)
      if [ "$tag" = "latest" ]; then
        echo "https://xget.xi-xu.me/gh/${owner}/${repo}/releases/latest/download/${filename}"
      else
        echo "https://xget.xi-xu.me/gh/${owner}/${repo}/releases/download/${tag}/${filename}"
      fi
      ;;
    jsdelivr)
      # jsdelivr CDN format
      if [ "$tag" = "latest" ]; then
        echo "https://cdn.jsdelivr.net/gh/${owner}/${repo}/releases/latest/download/${filename}"
      else
        echo "https://cdn.jsdelivr.net/gh/${owner}/${repo}@${tag}/${filename}"
      fi
      ;;
    statically)
      # statically CDN format
      if [ "$tag" = "latest" ]; then
        echo "https://cdn.statically.io/gh/${owner}/${repo}/releases/latest/download/${filename}"
      else
        echo "https://cdn.statically.io/gh/${owner}/${repo}/${tag}/${filename}"
      fi
      ;;
    *)
      echo "ERROR: Unknown proxy type: $proxy" >&2
      echo "Supported proxies: github, gh-proxy, xget, jsdelivr, statically" >&2
      exit 1
      ;;
  esac
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
# INSTALLATION FUNCTIONS
# ============================================================================

# Setup installation directory based on OS and permissions
# Sets global variables: EI_DIR, PATH_MODE (for Windows)
setup_install_dir() {
  local os_type="$1"
  if [ "$os_type" = "Windows" ]; then
    powershell -c "New-Item -Path '$EI_DIR' -ItemType Directory -Force | Out-Null"
  else
    # abs_path=$(resolve_path $EI_DIR)
    # mkdir -p $abs_path
    bash -c "mkdir -p $EI_DIR"
  fi
}

# Update PATH for Unix-like systems
# Args: install_dir
update_path_unix() {
  local install_dir="$1"
  local profile_file

  # Determine profile file
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
  local path_export="\n# Added by ${EI_BINARY_NAME} installer\nexport PATH=\"\$PATH:$install_dir\"\n"

  if [ -w "$profile_file" ] || [ ! -f "$profile_file" ]; then
    printf "%b" "$path_export" >> "$profile_file"
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

  # Extract OS type from Rust target triple for setup_install_dir
  case "$TARGET" in
    *-apple-darwin)
      OS_TYPE="Darwin"
      ;;
    *-pc-windows-*|*-windows-*)
      OS_TYPE="Windows"
      ;;
    *-linux-android|*-androideabi)
      OS_TYPE="Android"
      ;;
    *-linux-*)
      OS_TYPE="Linux"
      ;;
    *)
      OS_TYPE="Linux"
      ;;
  esac

  # Setup installation directory
  setup_install_dir "$OS_TYPE"
  local abs_path=$(resolve_path $EI_DIR)
  echo "Installation directory: $abs_path" $EI_DIR
  return

  # Create temporary download directory
  if command -v mktemp >/dev/null 2>&1; then
    DOWNLOAD_DIR="$(mktemp -d)"
  else
    DOWNLOAD_DIR="."
  fi

  DOWNLOAD_PATH="$DOWNLOAD_DIR/$FILENAME"

  # Generate download URL
  DOWNLOAD_URL="$(generate_download_url "$PROXY" "$EI_OWNER" "$EI_REPO" "$EI_TAG" "$FILENAME")"

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
  if [ "$OS_TYPE" = "Windows" ]; then
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
    if [ "$OS_TYPE" = "Windows" ]; then
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

  if [ "$OS_TYPE" = "Windows" ]; then
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
  if [ "$OS_TYPE" = "Windows" ]; then
    update_path_windows "$EI_DIR"
    win_path=$(resolve_windows_path $EI_DIR)
    add_to_github_path "$win_path"
  else
    update_path_unix "$EI_DIR"
    add_to_github_path "$abs_path"
  fi
}

main "$@"
