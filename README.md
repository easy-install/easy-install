# Easy Install

`ei` installs prebuilt tools from GitHub releases and other sources. It detects
the target platform, selects an asset, extracts archives, and installs the
result locally or over SSH.

## Install

**Windows**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://raw.githubusercontent.com/easy-install/easy-install/main/install.ps1 | iex"
```

**Linux / macOS**

```sh
curl -fsSL https://raw.githubusercontent.com/easy-install/easy-install/main/install.sh | sh
```

The install script also accepts options such as `--target`. Alternatively,
install with Cargo (`cargo install easy-install`) or npm
(`npm install -g @easy-install/easy-install`).

## Quick start

```sh
ei https://github.com/BurntSushi/ripgrep
ei sharkdp/fd
ei https://github.com/owner/repo/releases/tag/v1.2.3
ei https://example.com/tool.tar.gz
ei ./tool.zip
ei ./dist-manifest/tool.json
```

Sources include GitHub repositories and release URLs, direct URLs, local files,
dist-manifests, GitHub Actions artifacts, and `nightly.link` builds. Archives
are extracted automatically, including archives nested inside another archive.
Multi-file packages are installed in a named subdirectory under `~/.ei`.

## Select an asset

GitHub assets are matched to the local platform automatically. Use:

```sh
# Choose matching assets by name (comma-separated names are supported)
ei quickjs-ng/quickjs --name qjs

# Match the original asset filename with a regular expression
ei mpv-player/mpv --regex 'x86_64-pc-windows-msvc\.zip'

# Override platform detection, or allow arch/OS matching without ABI
ei owner/repo --target x86_64-unknown-linux-musl
ei owner/repo --fuzzy
```

`--name` filters platform-compatible assets. `--regex` applies directly to
GitHub asset filenames, bypasses platform matching, and must match exactly one
asset. Use `--alias` to rename the installed file/executable or, for a
multi-file archive, its install directory:

```sh
ei owner/repo --name tool --alias my-tool
```

## Destination and SSH

By default, files go to `~/.ei`, which is added to `PATH`. Use `-d` / `--dir`
to choose a local directory; `--skip-path` leaves `PATH` untouched.

```sh
ei owner/repo -d ~/.local/bin
```

To install remotely, provide an SSH destination as `-d` or as the second
positional argument. The artifact is downloaded locally and transferred over
SSH; remote platform detection is automatic when supported.

```sh
ei owner/repo -d user@host:/opt/tools -i ~/.ssh/id_ed25519
ei ./tool.tar.gz user@host:2222:/opt/tools --upx
```

Use `-i FILE` for a private key or `-p PASSWORD` for password authentication;
they are mutually exclusive. Specify the port with `-P PORT` (uppercase).
If remote platform detection is unavailable, pass `--target` explicitly.
`strip` and `upx` run on the destination host.

Before writing, `ei` checks available space when the host supports it. Existing
files that will be overwritten are deducted from the required space.

## Options

| Option | Purpose |
| --- | --- |
| `-d, --dir DIR` | Local install directory or SSH destination |
| `--name NAME` | Filter assets by name; comma-separated |
| `--regex PATTERN` | Select one GitHub asset by filename |
| `--target TARGET` | Override the target triple |
| `--fuzzy` | Match architecture and OS when ABI differs |
| `--alias NAME` | Rename a file/executable or multi-file install directory |
| `--skip-path` | Skip adding the install directory to PATH |
| `--strip` | Strip debug symbols from a single executable |
| `--upx` | Compress a single executable with UPX |
| `--proxy PROXY` | Use a GitHub download proxy |
| `--retry N`, `--timeout SECONDS` | Configure download retries and timeout |
| `-q, --quiet` | Suppress output |

`strip` and `upx` must be installed on the machine receiving the files.
When both are enabled, stripping runs first.

## Configuration and commands

Use `ei config` to view settings, or pass a key and value to change one:

```sh
ei config
ei config proxy gh-proxy
ei config dir ~/.local/bin
ei config target x86_64-unknown-linux-gnu
ei config timeout 300
ei config retry 5
ei config strip true
ei config upx true
```

Settings are stored in `ei_config.json` next to the `ei` executable. Supported
keys: `proxy`, `dir`, `target`, `timeout`, `retry`, `strip`, and `upx`.

```sh
ei upgrade
ei completions bash > ~/.local/share/bash-completion/completions/ei
```

`ei upgrade` updates the local executable and preserves its build target.
Completion scripts are available for bash, zsh, fish, PowerShell, and elvish.

## Dist-manifests

`ei` accepts local or remote manifests in the
[cargo-dist schema](https://github.com/axodotdev/cargo-dist/tree/main/cargo-dist-schema)
format. A manifest can describe release artifacts, target triples, and the
files in each package. See the [project manifests](https://github.com/easy-install/easy-install/tree/main/dist-manifest)
for examples.

## Cargo features

Default features are `cli` (the `ei` binary and its subcommands), `archive`
(the archive formats: `tar`, `tar-gz`, `tar-xz`, `tar-bz`, `tar-zstd`, `gz`,
`zip`) and `ssh` (remote installs). `cli` enables `archive`, `ssh` and
`logging`.

Library users that install locally only can disable the CLI and the remote
backend:

```toml
easy-install = { version = "1", default-features = false, features = ["tar-gz"] }
```

## License

MIT
