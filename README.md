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

## Installation

### Windows

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/easy-install/easy-install/releases/latest/download/install.ps1 | iex"
```

### Linux/macOS

```bash
# Direct installation
curl -fsSL https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh

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

# Set custom name and alias
ei https://github.com/ip7z/7zip/releases/tag/25.01 --name 7z2501 --alias 7z

# Install specific binary from a multi-binary package
ei https://github.com/quickjs-ng/quickjs --bin=qjs

# Install from a direct download URL
ei https://github.com/denoland/deno/releases/download/v2.1.1/deno-x86_64-pc-windows-msvc.zip
ei https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip
```

### Custom Installation Directory

```bash
# Install to a specific absolute path
ei ./dist-manifest/mpv-easy.json -d c:/mpv-easy

# Install to a named directory under ~/.ei/
ei ./dist-manifest/mpv-easy.json -d custom-name
```

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

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Repository

[https://github.com/easy-install/easy-install](https://github.com/easy-install/easy-install)
