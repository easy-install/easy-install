#!/bin/bash

set -e

RELEASE="latest"

# Parse optional arguments for OS and ARCH
if [ $# -ge 1 ]; then
  OS_ARG="$1"
fi
if [ $# -ge 2 ]; then
  ARCH_ARG="$2"
fi

# Detect OS if not provided
if [ -z "$OS_ARG" ]; then
  OS="$(uname -s)"
  case "${OS}" in
    MINGW* | Win*) OS="Windows" ;;
  esac
else
  OS="$OS_ARG"
fi

# Detect ARCH if not provided
if [ -z "$ARCH_ARG" ]; then
  ARCH="$(uname -m)"
else
  ARCH="$ARCH_ARG"
fi

set_filename() {
  if [ "$OS" = "Linux" ] || [ "$OS" = "Android" ]; then
    # Detect Android specifically if OS is Linux and not overridden
    if [ "$OS" = "Linux" ] && [ -z "$OS_ARG" ] && [ "$(uname -o 2>/dev/null)" = "Android" ]; then
      OS="Android"
    fi

    if [ "$OS" = "Android" ]; then
      case "$ARCH" in
        aarch64)
          FILENAME="ei-aarch64-linux-android.tar.gz"
          ;;
        *)
          echo "Unsupported architecture on Android: $ARCH"
          exit 1
      esac
    else
      # Standard Linux
      case "$ARCH" in
        arm | armv7*)
          FILENAME="ei-aarch32-unknown-linux-musl.tar.gz"
          ;;
        aarch* | armv8*)
          FILENAME="ei-aarch64-unknown-linux-musl.tar.gz"
          ;;
        *)
          FILENAME="ei-x86_64-unknown-linux-musl.tar.gz"
      esac
    fi
  elif [ "$OS" = "Darwin" ] ; then
    case "$ARCH" in
      arm64)
        FILENAME="ei-aarch64-apple-darwin.tar.gz"
        ;;
      x86_64)
        FILENAME="ei-x86_64-apple-darwin.tar.gz"
        ;;
      *)
        echo "Unsupported architecture on macOS: $ARCH"
        exit 1
    esac
  elif [ "$OS" = "Windows" ] ; then
    FILENAME="ei-x86_64-pc-windows-msvc.zip"
  else
    echo "OS $OS is not supported."
    echo "If you think that's a bug - please file an issue to https://github.com/easy-install/easy-install/issues"
    exit 1
  fi
}

check_dependencies() {
  echo "Checking dependencies for the installation script..."

  echo -n "Checking availability of curl... "
  if hash curl 2>/dev/null; then
    echo "OK!"
  else
    echo "Missing!"
    SHOULD_EXIT="true"
  fi

  if [ "$OS" = "Windows" ]; then
    echo -n "Checking availability of unzip... "
    if hash unzip 2>/dev/null; then
      echo "OK!"
    else
      echo "Missing!"
      SHOULD_EXIT="true"
    fi
  else
    echo -n "Checking availability of tar... "
    if hash tar 2>/dev/null; then
      echo "OK!"
    else
      echo "Missing!"
      SHOULD_EXIT="true"
    fi
  fi

  if [ "$SHOULD_EXIT" = "true" ]; then
    echo "Not installing ei due to missing dependencies."
    exit 1
  fi
}

ensure_containing_dir_exists() {
  if [ "$OS" = "Windows" ]; then
    powershell -c "New-Item -Path "~/.ei" -ItemType Directory -Force | Out-Null"
    INSTALL_DIR=$(powershell -c "[string](Resolve-Path ~/.ei)")
    is_admin=$(powershell -c "[bool]([Security.Principal.WindowsIdentity]::GetCurrent().Groups -match 'S-1-5-32-544')")
    if [ "$is_admin" = "True" ]; then
      mode="Machine"
    else
      mode="User"
    fi
    has_path=$(powershell -c "\$currentPath=[Environment]::GetEnvironmentVariable('Path', '$mode');[bool](\$currentPath -split ';' | Where-Object { \$_.ToLower() -eq '$INSTALL_DIR' })")
    if [ "$has_path" = "False" ]; then
      powershell -c "\$currentPath=[Environment]::GetEnvironmentVariable('Path', '$mode');\$newPath=\"\$currentPath;$INSTALL_DIR\"; [Environment]::SetEnvironmentVariable('Path', \$newPath, '$mode')"
    fi
  else
    mkdir -p $HOME/.ei
    INSTALL_DIR=$HOME/.ei
  fi
}

download() {
  if [ "$RELEASE" = "latest" ]; then
    URL="https://github.com/easy-install/easy-install/releases/latest/download/$FILENAME"
  else
    URL="https://github.com/easy-install/easy-install/releases/download/$RELEASE/$FILENAME"
  fi

  if command -v mktemp >/dev/null 2>&1; then
      DOWNLOAD_DIR=$(mktemp -d)
  else
      DOWNLOAD_DIR="."
  fi

  echo "Downloading $URL"

  if ! curl --progress-bar --fail -L "$URL" -o "$DOWNLOAD_DIR/$FILENAME"; then
    echo "Download failed. Check that the release/filename are correct."
    exit 1
  fi

  if [ "$OS" = "Windows" ]; then
    unzip -q "$DOWNLOAD_DIR/$FILENAME" -d "$DOWNLOAD_DIR"
    mv "$DOWNLOAD_DIR/ei" "$INSTALL_DIR/ei.exe"
    chmod u+x "$INSTALL_DIR/ei.exe"
    echo "Successfully installed to $INSTALL_DIR/ei.exe"
  else
    tar -xzf "$DOWNLOAD_DIR/$FILENAME" -C "$DOWNLOAD_DIR"
    mv "$DOWNLOAD_DIR/ei" "$INSTALL_DIR/ei"
    chmod u+x "$INSTALL_DIR/ei"
    echo "Successfully installed to $INSTALL_DIR/ei"
  fi
}

add_to_github(){
  if [ "$GITHUB_ACTIONS" = "true" ]; then
      echo $INSTALL_DIR >> "$GITHUB_PATH"
      echo "Added $INSTALL_DIR to GITHUB_PATH"
  fi
}

add_to_profile() {
  local profile="/etc/profile"

  if grep -q "$INSTALL_DIR" "$profile"; then
    echo "$INSTALL_DIR already in PATH"
  else
    echo "" >> "$profile"
    echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$profile"
    echo "" >> "$profile"
    echo "Added $INSTALL_DIR to $profile"
  fi
}


set_filename
check_dependencies
ensure_containing_dir_exists
download
add_to_github
add_to_profile