#!/bin/bash

set -e

RELEASE="latest"
OS="$(uname -s)"

case "${OS}" in
   MINGW* | Win*) OS="Windows" ;;
esac



set_filename() {
  if [ "$OS" = "Linux" ]; then
    # Based on https://stackoverflow.com/a/45125525
    case "$(uname -m)" in
      arm | armv7*)
        FILENAME="ei-aarch32-unknown-linux-musl.tar.gz"
        ;;
      aarch* | armv8*)
        FILENAME="ei-aarch64-unknown-linux-musl.tar.gz"
        ;;
      *)
        FILENAME="ei-x86_64-unknown-linux-musl.tar.gz"
    esac
  elif [ "$OS" = "Darwin" ] ; then
    FILENAME="ei-aarch64-apple-darwin.tar.gz"
    echo "Downloading the latest binary from GitHub..."
  elif [ "$OS" = "Windows" ] ; then
    FILENAME="ei-x86_64-pc-windows-gnu.zip"
    echo "Downloading the latest binary from GitHub..."
  else
    echo "OS $OS is not supported."
    echo "If you think that's a bug - please file an issue to https://github.com/ahaoboy/easy-install/issues"
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

  echo -n "Checking availability of tar... "
  if hash tar 2>/dev/null; then
    echo "OK!"
  else
    echo "Missing!"
    SHOULD_EXIT="true"
  fi

  echo -n "Checking availability of unzip... "
  if hash unzip 2>/dev/null; then
    echo "OK!"
  else
    echo "Missing!"
    SHOULD_EXIT="true"
  fi

  if [ "$SHOULD_EXIT" = "true" ]; then
    echo "Not installing ei due to missing dependencies."
    exit 1
  fi
}

ensure_containing_dir_exists() {
  if [ "$OS" = "Windows" ]; then
    powershell -c "New-Item -Path "~/easy-install" -ItemType Directory -Force"
    INSTALL_DIR=$(powershell -c "[string](Resolve-Path ~/easy-install)")
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
    mkdir -p /usr/local/bin
    INSTALL_DIR="/usr/local/bin"
  fi
}


download() {
  if [ "$RELEASE" = "latest" ]; then
    URL="https://github.com/ahaoboy/easy-install/releases/latest/download/$FILENAME"
  else
    URL="https://github.com/ahaoboy/easy-install/releases/download/$RELEASE/$FILENAME"
  fi

  DOWNLOAD_DIR=$(mktemp -d)

  echo "Downloading $URL..."

  if ! curl --progress-bar --fail -L "$URL" -o "$DOWNLOAD_DIR/$FILENAME"; then
    echo "Download failed.  Check that the release/filename are correct."
    exit 1
  fi

  if [ "$OS" = "Windows" ]; then
    unzip -q "$DOWNLOAD_DIR/$FILENAME" -d "$DOWNLOAD_DIR"
    mv "$DOWNLOAD_DIR/ei" "$INSTALL_DIR/ei.exe"
    chmod u+x "$INSTALL_DIR/ei.exe"
  else
    tar -xzf "$DOWNLOAD_DIR/$FILENAME" -C "$DOWNLOAD_DIR"
    mv "$DOWNLOAD_DIR/ei" "$INSTALL_DIR/ei"
    chmod u+x "$INSTALL_DIR/ei"
  fi

  echo "successfully installed to $INSTALL_DIR"
}

set_filename
check_dependencies
ensure_containing_dir_exists
download
