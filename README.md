# Easy Install

A cross-platform command-line tool for effortlessly installing binaries from GitHub releases and other sources. Simplify your workflow by downloading and setting up executables with a single command.

## Features

- 🚀 Install binaries directly from GitHub releases
- 🎯 Automatic platform detection and binary selection
- 📦 Support for multiple archive formats (zip, tar.gz, tar.xz, etc.)
- 🔧 Custom installation directories
- 🌐 Proxy support for restricted networks
- 📋 Manifest-based installations for complex packages
- 🔄 Version-specific or latest release installation
- 💾 Automatic PATH configuration
- ⚙️ Persistent configuration management
- ⏱️ Configurable network timeouts
- 🗜️ Binary optimization with strip and UPX compression
- 🔄 Self-upgrade support

## Installation

### Windows

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/easy-install/easy-install/releases/latest/download/install.ps1 | iex"
```

### Linux/macOS

```bash
# Direct installation
curl -fsSL https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh

# wget
wget -qO- https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh

# Using proxy (for restricted networks)
curl -fsSL https://gh-proxy.com/https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh

# Using CDN
curl -fsSL https://cdn.jsdelivr.net/gh/easy-install/easy-install/install.sh | sh

# Using xget proxy
curl -fsSL https://xget.xi-xu.me/gh/easy-install/easy-install/raw/refs/heads/main/install.sh | sh -s -- --proxy xget

# Using gh-proxy
curl -fsSL https://gh-proxy.com/https://github.com/easy-install/easy-install/blob/main/install.sh | sh -s -- --proxy gh-proxy

# Using ei-assets mirror (more proxy options available)
curl -fsSL https://cdn.jsdelivr.net/gh/ahaoboy/ei-assets/install.sh | sh -s -- --proxy jsdelivr
```

### Cargo (Rust)

```bash
# Install from crates.io
cargo install easy-install

# Install using cargo-binstall
cargo binstall easy-install

# Install from GitHub
cargo install --git https://github.com/easy-install/easy-install.git
```

### npm/pnpm/yarn

```bash
npm install -g @easy-install/easy-install
# or
pnpm add -g @easy-install/easy-install
# or
yarn global add @easy-install/easy-install
```

## Usage

### Basic Installation

```bash
# Install the latest release from a GitHub repository
ei https://github.com/ahaoboy/mujs-build

# Install a specific version
ei https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.1

# Short syntax for GitHub repositories
ei yt-dlp/yt-dlp
```

### Advanced Options

```bash
# Specify target platform
ei https://github.com/ahaoboy/mujs-build --target x86_64-pc-windows-gnu

# Set a custom alias for the installed binary (or directory, for multi-file packages)
ei https://github.com/ip7z/7zip/releases/tag/25.01 --alias 7z

# Install specific binary from a multi-binary package
ei https://github.com/quickjs-ng/quickjs --alias=qjs

# Combine --regex and --alias for complex filenames
ei mpv-player/mpv@git-release --regex "x86_64-pc-windows-msvc\.zip" --alias mpv-dev

# Install from a direct download URL
ei https://github.com/denoland/deno/releases/download/v2.1.1/deno-x86_64-pc-windows-msvc.zip
ei https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip

# Install without adding to PATH
ei https://github.com/quickjs-ng/quickjs --no-path

# Optimize binary with strip and UPX compression
ei https://github.com/boa-dev/boa --strip --upx
```

### Filtering Artifacts: --name vs --regex

When a GitHub release contains multiple assets, `ei` automatically detects your platform and picks the right one. But sometimes you need finer control — that's where `--name` and `--regex` come in.

#### --name

`--name` is used when there are multiple assets matching your platform, and you only want one of them. For example, the [quickjs-ng](https://github.com/quickjs-ng/quickjs) release contains both `qjs` and `qjsc` executables:

```bash
# Both qjs and qjsc match your platform — pick just qjs
ei quickjs-ng/quickjs --name qjs
```

**How it works:** `ei` first uses `guess_target` to find all assets matching your platform (e.g., `x86_64-pc-windows-msvc`), then applies `--name` to filter the results. It does NOT bypass platform detection.

#### --regex

`--regex` is used when asset filenames are too complex for `guess_target` to extract platform information. For example, [mpv](https://github.com/mpv-player/mpv) uses filenames with embedded version hashes:

```
mpv-v0.41.0-dev-g4c220ffd9-28826186115-x86_64-pc-windows-msvc.zip
mpv-v0.41.0-dev-g4c220ffd9-28826186115-aarch64-pc-windows-msvc.zip
```

These confuse `guess_target` because the version blob sits between the tool name and the platform triple. Use `--regex` to match the raw filename directly:

```bash
# Match the x86_64 windows-msvc asset by its filename
ei mpv-player/mpv@git-release --regex "x86_64-pc-windows-msvc\.zip"

# Match a macOS variant
ei mpv-player/mpv@git-release --regex "macos-15-arm"
```

**How it works:** The regex is matched directly against each asset's original filename. When a match is found, the asset is selected immediately — `guess_target` is completely bypassed. The regex **must match exactly one asset**; matching zero or multiple is an error.

**Pairing with `--alias`:** Regex-matched assets often have long, unwieldy names. Use `--alias` to give the installed file (or directory, for multi-file packages) a clean, memorable name:

```bash
# Single-file package: the executable is renamed
# long-name-x86_64-pc-windows-msvc.exe → tool.exe
ei some/tool --regex "x86_64-pc-windows-msvc" --alias tool

# Multi-file package (e.g. mpv): the install directory is renamed
# ~/.ei/long-dir-name/ → ~/.ei/mpv-dev/
ei mpv-player/mpv@git-release --regex "x86_64-pc-windows-msvc\.zip" --alias mpv-dev
```

> 💡 **Rule of thumb:** Use `--name` when `guess_target` can recognize your platform in the filenames. Use `--regex` when filenames are non-standard and platform detection fails.

### CLI Reference

Below is the full list of arguments and options accepted by `ei`:

```
USAGE:
    ei [OPTIONS] [URL] [COMMAND]
```

#### Arguments

| Argument | Description                                                                        |
| -------- | ---------------------------------------------------------------------------------- |
| `[URL]`  | GitHub repo (`owner/repo`), release URL, or artifact URL. If omitted, prints help. |

#### Options

| Option                | Short | Description                                                                                                | Default     |
| --------------------- | ----- | ---------------------------------------------------------------------------------------------------------- | ----------- |
| `--dir <DIR>`         | `-d`  | Installation directory for downloaded binaries. Can be an absolute path or a name (stored under `~/.ei/`). | `~/.ei`     |
| `--no-path`           |       | Skip adding installed binaries to PATH.                                                                    | `false`     |
| `--name <NAME>`       |       | Filter artifacts by name. Supports comma-separated values for multiple filters.                            |             |
| `--alias <ALIAS>`     |       | Rename the installed binary (or directory, for multi-file packages).                                       |             |
| `--target <TARGET>`   |       | Target platform (e.g., `x86_64-unknown-linux-gnu`). Auto-detected if not specified.                        | auto-detect |
| `--retry <N>`         |       | Number of retry attempts for failed downloads.                                                             | `3`         |
| `--proxy <PROXY>`     |       | GitHub proxy to use (`github`, `gh-proxy`, `ghproxy`, `jsdelivr`, etc.).                                   | `github`    |
| `--timeout <SECONDS>` |       | Network request timeout in seconds.                                                                        | `600`       |
| `--strip [BOOL]`      |       | Strip debug symbols from executable. Can be used as a flag (`--strip`) or with a value (`--strip true`).   | `false`     |
| `--upx [BOOL]`        |       | Compress executable with UPX. Can be used as a flag (`--upx`) or with a value (`--upx true`).              | `false`     |
| `--quiet`             | `-q`  | Suppress all output messages.                                                                              | `false`     |
| `--fuzzy`             |       | Use fuzzy target matching (match arch+os, ignoring abi).                                                   | `false`     |
| `--regex <PATTERN>`   |       | Match asset filenames directly with regex, bypassing platform detection. See [Filtering Artifacts](#filtering-artifacts---name-vs---regex). |             |
| `--version`           | `-V`  | Print version information.                                                                                 |             |
| `--help`              | `-h`  | Print help information.                                                                                    |             |

#### Subcommands

| Subcommand            | Description                                                                                          |
| --------------------- | ---------------------------------------------------------------------------------------------------- |
| `config`              | Manage persistent configuration settings. See [Configuration Management](#configuration-management). |
| `completions <SHELL>` | Generate shell completion scripts. See [Shell Completions](#shell-completions).                      |
| `upgrade`             | Upgrade `ei` to the latest version.                                                                  |

### Binary Optimization

Easy Install supports automatic binary optimization for single-executable installations:

```bash
# Strip debug symbols to reduce binary size
ei https://github.com/boa-dev/boa --strip

# Compress binary with UPX for maximum size reduction
ei https://github.com/boa-dev/boa --upx

# Combine both for optimal results (strip runs first, then UPX)
ei https://github.com/boa-dev/boa --strip --upx
# 27M boa-x86_64-pc-windows-msvc.exe -> 7.1M C:/Users/Admin/.ei/boa.exe
```

**Requirements:**
- `--strip`: Requires the `strip` command to be available in PATH
- `--upx`: Requires the `upx` command to be available in PATH

**Notes:**
- Optimization only works when installing a single executable file
- If the required tool is not found, a warning is displayed but installation continues
- Works cross-platform: you can use these flags on any OS, and they'll be silently skipped if tools aren't available
- UPX uses `--best --lzma` flags for maximum compression

### Custom Installation Directory

```bash
# Install to a specific absolute path
ei ./dist-manifest/mpv-easy.json -d c:/mpv-easy

# Install to a named directory under ~/.ei/
ei ./dist-manifest/mpv-easy.json -d custom-name
```

### Upgrade

Upgrade `ei` itself to the latest version with a single command:

```bash
ei upgrade
```

This will download the latest release from the official repository and replace the current binary in-place.

### Configuration Management

Easy Install supports persistent configuration through the `config` subcommand. Configuration is stored in `ei_config.json` in the same directory as the `ei` executable.

```bash
# View all configuration
ei config

# View specific configuration
ei config proxy
ei config dir
ei config target
ei config timeout
ei config retry
ei config strip
ei config upx

# Set configuration values
ei config proxy gh-proxy
ei config dir /custom/install/path
ei config target x86_64-pc-windows-msvc
ei config timeout 300
ei config retry 5
ei config strip true
ei config upx true

# Configuration priority (highest to lowest):
# 1. Command-line arguments (--proxy, --dir, --target, --timeout, --retry, --strip, --upx)
# 2. Configuration file (ei_config.json)
# 3. Default values
```

**Supported Configuration Keys:**
- `proxy` - Default proxy for GitHub downloads (github, gh-proxy, ghproxy, jsdelivr, etc.)
- `dir` - Default installation directory
- `target` - Default target platform
- `timeout` - Network request timeout in seconds (default: 600)
- `retry` - Number of retry attempts for failed downloads (default: 3)
- `strip` - Strip debug symbols from executables (default: false)
- `upx` - Compress executables with UPX (default: false)

### Quiet Mode

Suppress all output messages during installation:

```bash
# Install silently (no output)
ei https://github.com/ahaoboy/mujs-build --quiet

# Short form
ei https://github.com/ahaoboy/mujs-build -q

# Useful for scripts and automation
ei yt-dlp/yt-dlp -q && echo "Installation complete"
```

### Shell Completions

Generate shell completion scripts for your preferred shell:

```bash
# Generate completions for bash
ei completions bash > ~/.local/share/bash-completion/completions/ei

# Generate completions for zsh
ei completions zsh > ~/.zfunc/_ei

# Generate completions for fish
ei completions fish > ~/.config/fish/completions/ei.fish

# Generate completions for PowerShell
ei completions powershell > $PROFILE/../Completions/ei.ps1

# Generate completions for elvish
ei completions elvish > ~/.config/elvish/completions/ei.elv
```

**Supported Shells:**
- bash
- zsh
- fish
- powershell
- elvish

After generating the completion script, restart your shell or source the completion file to enable tab completion for `ei` commands.

**Configuration File Location:**
- The `ei_config.json` file is created in the same directory as the `ei` executable
- Only created when you use `ei config` commands
- If the file exists but is corrupted, it will be automatically reset to defaults

### Manifest-Based Installation

```bash
# Install from a remote manifest
ei "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.4/dist-manifest.json"
ei "https://github.com/easy-install/easy-install/releases/latest/download/ffmpeg.json"

# Install from a local manifest file
ei "./dist-manifest/screentogif.json"
```

## Use Cases

### Replacing cargo-binstall

Easy Install can be used as a drop-in replacement for cargo-binstall with more flexibility:

```bash
# Instead of: cargo binstall cargo-binstall
# Use:
ei cargo-bins/cargo-binstall -d ~/.cargo/bin

# This installs cargo-binstall to your Cargo bin directory
# Works with any GitHub release, not just Rust projects
```

## Distribution Manifest

For complex packages containing multiple files, you can create a `dist-manifest.json` file to define the structure and assets. This follows the [cargo-dist-schema](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist-schema) format.

### Example: mujs

A typical mujs release contains multiple files:

```
.
├── libmujs.a
├── libmujs.o
├── libmujs.so
├── mujs-pp.exe
├── mujs.exe
└── mujs.pc
```

The corresponding [dist-manifest.json](https://github.com/ahaoboy/mujs-build/blob/main/dist-manifest.json) defines which files to install:

```json
{
  "mujs-aarch64-apple-darwin.tar.gz": {
    "name": "mujs-aarch64-apple-darwin.tar.gz",
    "target_triples": [
      "aarch64-apple-darwin"
    ],
    "assets": [
      {
        "name": "mujs",
        "path": "mujs",
        "kind": "executable"
      },
      {
        "name": "mujs-pp",
        "path": "mujs-pp",
        "kind": "executable"
      },
      {
        "name": "libmujs.dylib",
        "path": "libmujs.dylib",
        "kind": "c_dynamic_library"
      },
      {
        "name": "libmujs.a",
        "path": "libmujs.a",
        "kind": "c_static_library"
      }
    ]
  }
}
```

### Example: Zig

For tools hosted outside GitHub, you can specify direct download URLs:

```json
{
  "artifacts": {
    "https://ziglang.org/download/0.13.0/zig-linux-x86_64-0.13.0.tar.xz": {
      "name": "zig",
      "target_triples": ["x86_64-unknown-linux-gnu"]
    },
    "https://ziglang.org/download/0.13.0/zig-macos-x86_64-0.13.0.tar.xz": {
      "name": "zig",
      "target_triples": ["x86_64-apple-darwin"]
    },
    "https://ziglang.org/download/0.13.0/zig-macos-aarch64-0.13.0.tar.xz": {
      "name": "zig",
      "target_triples": ["aarch64-apple-darwin"]
    },
    "https://ziglang.org/download/0.13.0/zig-windows-x86_64-0.13.0.zip": {
      "name": "zig",
      "target_triples": ["x86_64-pc-windows-gnu"]
    }
  }
}
```

## Supported Platforms

- Windows (x86_64, aarch64)
- Linux (x86_64, aarch64, musl)
- macOS (x86_64, aarch64/Apple Silicon)

## Default Installation Location

Binaries are installed to `~/.ei` by default, which is automatically added to your PATH during installation.

## Similar Tools

- [eget](https://github.com/zyedidia/eget) - Easily install prebuilt binaries from GitHub
- [ubi](https://github.com/houseabsolute/ubi) - Universal Binary Installer
- [dra](https://github.com/devmatteini/dra) - Download release assets from GitHub

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Repository

[https://github.com/easy-install/easy-install](https://github.com/easy-install/easy-install)
