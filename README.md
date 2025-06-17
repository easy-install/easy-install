## install

### windows

```bash
powershell -ExecutionPolicy Bypass -c "irm https://github.com/easy-install/easy-install/releases/latest/download/install.ps1 | iex"
```

### linux/macos

```bash
curl -fsSL https://github.com/easy-install/easy-install/releases/latest/download/install.sh | bash
```

### cargo

```bash
cargo install --git https://github.com/easy-install/easy-install.git

cargo binstall easy-install

cargo install easy-install
```

### npm

```bash
npm i @easy-install/easy-install -g
```

## usage

```bash
# install latest
ei https://github.com/ahaoboy/mujs-build

# install v0.0.1
ei https://github.com/ahaoboy/mujs-build/releases/tag/v0.0.1

# install yt-dlp from github
ei ansi2 yt-dlp/yt-dlp

# install deno and denort
ei https://github.com/denoland/deno

# install deno
ei https://github.com/denoland/deno/releases/download/v2.1.1/deno-x86_64-pc-windows-msvc.zip
ei https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip

# install from json
ei "https://github.com/ahaoboy/mujs-build/releases/download/v0.0.4/dist-manifest.json"
ei "https://github.com/easy-install/easy-install/releases/latest/download/ffmpeg.json"

# install from local json file
ei "./dist-manifest/screentogif.json"

# install to custom directory
cargo run -- ./dist-manifest/mpv-easy.json -d c:/mpv-easy

# install to custom directory name (~/.ei/custom-name)
cargo run -- ./dist-manifest/mpv-easy.json -d custom-name

# only install qjs
cargo run -- https://github.com/quickjs-ng/quickjs --bin=qjs
```

## dist-manifest.json

### mujs

When the release package contains many files, add a dist-manifest.json file to
define the format of each file

[cargo-dist-schema](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist-schema)

Taking mujs as an example, it contains the following files

```
.
├── libmujs.a
├── libmujs.o
├── libmujs.so
├── mujs-pp.exe
├── mujs.exe
└── mujs.pc
```

[dist-manifest.json](https://github.com/ahaoboy/mujs-build/blob/main/dist-manifest.json)

```
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
},
```

### zig

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
