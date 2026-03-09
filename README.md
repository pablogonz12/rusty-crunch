# 🔧 rusty-crunch

Fast, parallel, terminal-based media converter written in Rust. Batch-compress audio, video, images, and documents with an interactive CLI.

## Features

- **Parallel processing** — uses all CPU cores via rayon
- **Recommended Crunch** — one-click optimal compression for all your files
- **Agent Mode [BETA]** — background service that auto-converts new files as they appear
- **Smart format filtering** — only shows output formats that make sense (no MP3→WAV)
- **Auto-installs dependencies** — detects your package manager and installs missing tools
- **Persistent settings** — configure defaults once, reuse everywhere
- **Display modes** — Verbose (troubleshooting) or Clean (tidy screen)
- **Cross-platform** — Linux, macOS, and Windows

| Category  | Supported formats                                    |
|-----------|------------------------------------------------------|
| 🎵 Audio  | MP3, WAV, OGG, FLAC, AAC, M4A, WMA, OPUS           |
| 🎬 Video  | MP4, MKV, AVI, MOV, WEBM, FLV, WMV, TS             |
| 🖼️ Images | PNG, JPEG, BMP, GIF, WEBP, TIFF, AVIF, ICO         |
| 📄 Docs   | PDF, DOCX, XLSX, PPTX, ODT, ODS, ODP, EPUB         |

## Installation

### Option A: Download pre-built binary

Download the latest release for your OS from the [Releases page](https://github.com/pablogonz12/rusty-crunch/releases).

| OS      | File                                     |
|---------|------------------------------------------|
| Linux   | `rusty-crunch-linux-x86_64`              |
| macOS   | `rusty-crunch-macos-x86_64`              |
| Windows | `rusty-crunch-windows-x86_64.exe`        |

```bash
# Linux / macOS — make it executable
chmod +x rusty-crunch-*
./rusty-crunch-linux-x86_64
```

### Option B: Build from source

```bash
# Install Rust: https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/pablogonz12/rusty-crunch.git
cd rusty-crunch
cargo build --release
# Binary: ./target/release/rusty-crunch
```

### Option C: Install via Cargo

```bash
cargo install --path .
# Now `rusty-crunch` is available from any terminal
```

## Required external tools

rusty-crunch is a **front-end** that orchestrates these excellent open-source tools. They must be installed on your system — rusty-crunch will **auto-install** them if a supported package manager is found.

| Tool         | Used for          | License              | Website                                    |
|--------------|-------------------|----------------------|--------------------------------------------|
| **FFmpeg**       | Audio & Video     | LGPL 2.1 / GPL      | [ffmpeg.org](https://ffmpeg.org)           |
| **ImageMagick**  | Images            | Apache 2.0           | [imagemagick.org](https://imagemagick.org) |
| **Ghostscript**  | PDF compression   | AGPL 3.0             | [ghostscript.com](https://ghostscript.com) |
| **LibreOffice**  | Document convert  | MPL 2.0              | [libreoffice.org](https://libreoffice.org) |

> **Note:** These tools are NOT bundled with the rusty-crunch binary — they are called as external processes. rusty-crunch will attempt to auto-install them on first use via your system package manager (apt, dnf, pacman, brew, winget, choco, scoop).

<details>
<summary><strong>Manual install — Linux</strong></summary>

```bash
# Fedora/RHEL
sudo dnf install ffmpeg ImageMagick ghostscript libreoffice

# Debian/Ubuntu
sudo apt-get install ffmpeg imagemagick ghostscript libreoffice

# Arch
sudo pacman -S ffmpeg imagemagick ghostscript libreoffice-fresh
```
</details>

<details>
<summary><strong>Manual install — macOS</strong></summary>

```bash
brew install ffmpeg imagemagick ghostscript libreoffice
```
</details>

<details>
<summary><strong>Manual install — Windows</strong></summary>

```powershell
# winget
winget install Gyan.FFmpeg ImageMagick.ImageMagick ArtifexSoftware.GhostScript TheDocumentFoundation.LibreOffice

# or Chocolatey
choco install ffmpeg imagemagick ghostscript libreoffice-fresh

# or Scoop
scoop install ffmpeg imagemagick ghostscript libreoffice
```
</details>

## Usage

```bash
rusty-crunch                    # interactive mode
rusty-crunch ~/Music            # skip folder picker
rusty-crunch --dry-run ~/Music  # preview without converting
rusty-crunch --agent-status     # check if the background agent is running
rusty-crunch --agent-stop       # stop the background agent
```

The main menu offers:

```
? What would you like to do?
> 🔧 Start Crunching
  🚀 Recommended Crunch
  🤖 Agent Mode [BETA]
  ⚙️  Settings
  🚪 Exit
```

### Start Crunching

Step-by-step guided conversion: pick media type → input format → output format → folder → options → go.

### Recommended Crunch

One-click optimal conversion for every supported file in a folder:

| Input                     | Output         | Rationale                    |
|---------------------------|----------------|------------------------------|
| WAV                       | FLAC           | Lossless, ~60% smaller       |
| MP3, OGG, AAC, M4A, WMA  | OPUS           | Best lossy codec             |
| AVI, MOV, FLV, WMV, TS   | MKV            | Modern container, H.264      |
| BMP, TIFF, ICO, GIF       | PNG            | Lossless compression         |
| JPEG                      | AVIF           | Best lossy image codec       |
| PDF                       | PDF (Optimized)| 150 PPI image downsampling   |

### Agent Mode [BETA]

A background service that watches folders and auto-converts files matching your rules. Set up rules through the interactive menu, then the agent runs as a detached background process that survives terminal close.

```bash
# Set up rules through the interactive menu first, then:
rusty-crunch --agent-status     # see if agent is running
rusty-crunch --agent-stop       # stop the agent
```

**Trigger modes:**
- **Watch** — converts files as soon as they appear (uses OS file system notifications)
- **Periodic** — scans folders at a configurable interval (default: 5 minutes)

> **Note:** Agent Mode is **BETA** on Linux and Windows, **ALPHA** on macOS. File system watching behavior varies by OS. The agent writes logs to `~/.config/rusty-crunch/agent.log`.

### Settings

Persisted to `~/.config/rusty-crunch/config.json` (Linux/macOS) or `%APPDATA%\rusty-crunch\config.json` (Windows):

- Default recursive scan (yes/no)
- Default delete originals (yes/no)
- Default folder path
- Display mode (Verbose / Clean)

## Project structure

```
rusty-crunch/
├── Cargo.toml
├── src/
│   ├── main.rs         # Entry point, main menu, recommended crunch
│   ├── agent.rs        # Agent Mode — background auto-conversion service [BETA]
│   ├── config.rs       # Persistent settings + agent rules
│   ├── formats.rs      # Media types, format defs, compatibility map
│   ├── prompt.rs       # Interactive prompts (dialoguer)
│   ├── converter.rs    # Conversion via external tools
│   ├── processor.rs    # Parallel batch processing (rayon + indicatif)
│   ├── deps.rs         # Auto-detect PM & install missing tools
│   └── util.rs         # Shared utilities (cores, has, H.264 encoder detection)
├── LICENSE
└── README.md
```

## Acknowledgments

rusty-crunch is made possible by these open-source projects:

**External tools:**
- [FFmpeg](https://ffmpeg.org) — Audio/video encoding and decoding
- [ImageMagick](https://imagemagick.org) — Image format conversion
- [Ghostscript](https://ghostscript.com) — PDF processing and optimization
- [LibreOffice](https://libreoffice.org) — Document format conversion

**Rust crates:**
- [clap](https://crates.io/crates/clap) — Command-line argument parsing
- [dialoguer](https://crates.io/crates/dialoguer) — Interactive terminal prompts
- [console](https://crates.io/crates/console) — Terminal colors and control
- [rayon](https://crates.io/crates/rayon) — Parallel processing
- [indicatif](https://crates.io/crates/indicatif) — Progress bars
- [walkdir](https://crates.io/crates/walkdir) — Directory traversal
- [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) — Config serialization
- [dirs](https://crates.io/crates/dirs) — Platform-appropriate config paths
- [notify](https://crates.io/crates/notify) — File system watching (Agent Mode)
- [ctrlc](https://crates.io/crates/ctrlc) — Graceful signal handling
- [anyhow](https://crates.io/crates/anyhow) — Error handling

## License

MIT — see [LICENSE](LICENSE).
