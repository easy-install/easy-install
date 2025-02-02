$ErrorActionPreference = "Stop"

$RELEASE = "latest"
$FILENAME = "ei-x86_64-pc-windows-gnu.zip"
$HOME_PATH = (Resolve-Path "~").Path
$INSTALL_DIR = Join-Path -Path $HOME_PATH -ChildPath ".easy-install"

function Ensure-ContainingDirExists {
  if (-not (Test-Path -Path $INSTALL_DIR)) {
    New-Item -Path $INSTALL_DIR -ItemType Directory -Force | Out-Null
  }

  $currentPath = [Environment]::GetEnvironmentVariable('Path', 'User')
  if (-not ($currentPath -split ';' | Where-Object { $_ -eq $INSTALL_DIR })) {
      [Environment]::SetEnvironmentVariable('Path', "$currentPath;$INSTALL_DIR", 'User')
  }
}

function Download {
    if ($RELEASE -eq "latest") {
        $URL = "https://github.com/ahaoboy/easy-install/releases/latest/download/$FILENAME"
    } else {
        $URL = "https://github.com/ahaoboy/easy-install/releases/download/$RELEASE/$FILENAME"
    }

    $tempFile = [System.IO.Path]::GetTempFileName()+(Get-Random)
    if (Test-Path -Path $tempFile) {
      Remove-Item -Path $tempFile -Force -Recurse
    }
    $DOWNLOAD_DIR=New-Item -ItemType Directory -Path $tempFile

    Write-Host "Downloading $URL..."

    try {
        Invoke-WebRequest -Uri $URL -OutFile "$DOWNLOAD_DIR/$FILENAME" -UseBasicP -ErrorAction Stop
    } catch {
        Write-Host "Download failed. Check that the release/filename are correct."
        exit 1
    }

    Expand-Archive -Path "$DOWNLOAD_DIR\$FILENAME" -DestinationPath $DOWNLOAD_DIR -Force
    Move-Item -Path "$DOWNLOAD_DIR\ei.exe" -Destination "$INSTALL_DIR\ei.exe" -Force
    Write-Host "Successfully installed to $INSTALL_DIR/ei.exe"
}

Ensure-ContainingDirExists
Download