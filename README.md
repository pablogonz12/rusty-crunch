# 🔧 rusty-crunch

A lightning-fast, asynchronous, terminal-based media converter and background agent built in Rust. Batch-compress audio, video, images, and documents locally without uploading anything to the cloud.

## Features

- **Asynchronous I/O** — leverages the `tokio` runtime for highly concurrent, non-blocking file processing (zero-copy optimizations).
- **Smart Caching** — features an atomic, append-only cache using `DashMap` to instantly skip unchanged or already-optimized files across sessions.
- **Recommended Crunch** — one-click optimal compression pipelines (including lossless AIFF/WAV to FLAC).
- **Agent Mode** — a automated background service that continuously watches folders and auto-converts new files as they appear.
- **Multi-job Workflows** — handles subfolder replication and complex batch outputs effortlessly.
- **Tracing & Logging** — built-in structured logging framework for deep diagnostics (`agent.log`).
- **Auto-installs dependencies** — detects your package manager and installs missing external tools.
- **Cross-platform** — statically linked native binaries for Linux (musl), macOS, and Windows.

| Category  | Supported formats                                    |
|-----------|------------------------------------------------------|
| 🎵 Audio  | AIFF, FLAC, WAV, MP3, OGG, AAC, M4A, WMA, OPUS       |
| 🎬 Video  | MP4, MKV, AVI, MOV, WEBM, FLV, WMV, TS               |
| 🖼️ Images | PNG, JPEG, BMP, GIF, WEBP, TIFF, AVIF, ICO           |
| 📄 Docs   | PDF, DOCX, XLSX, PPTX, ODT, ODS, ODP, EPUB           |

## Installation

### Option A: Download pre-built binary

Download the latest release for your OS from the [Releases page](https://github.com/pablogonz12/rusty-crunch/releases).

| OS      | File                                     |
|---------|------------------------------------------|
| Linux   | `rusty-crunch-linux-x86_64`              |
| macOS   | `rusty-crunch-macos-x86_64` (and `aarch64`)|
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

## Required external tools

rusty-crunch is a **front-end** that orchestrates these excellent open-source tools. They must be installed on your system — rusty-crunch will **auto-install** them if a supported package manager is found.

| Tool         | Used for          | License              | Website                                    |
|--------------|-------------------|----------------------|--------------------------------------------|
| **FFmpeg**       | Audio & Video     | LGPL 2.1 / GPL      | [ffmpeg.org](https://ffmpeg.org)           |
| **ImageMagick**  | Images            | Apache 2.0           | [imagemagick.org](https://imagemagick.org) |
| **Ghostscript**  | PDF compression   | AGPL 3.0             | [ghostscript.com](https://ghostscript.com) |
| **LibreOffice**  | Document convert  | MPL 2.0              | [libreoffice.org](https://libreoffice.org) |

## Usage

```bash
rusty-crunch                    # interactive mode
rusty-crunch ~/Music            # skip folder picker
rusty-crunch --dry-run ~/Music  # preview without converting
rusty-crunch --agent-status     # check if the background agent is running
rusty-crunch --agent-stop       # stop the background agent
```

### Recommended Crunch

One-click optimal conversion for every supported file in a folder:

| Input                     | Output         | Rationale                    |
|---------------------------|----------------|------------------------------|
| WAV, AIFF                 | FLAC           | Lossless, ~60% smaller       |
| MP3, OGG, AAC, M4A, WMA  | OPUS           | Best lossy codec             |
| AVI, MOV, FLV, WMV, TS   | MKV            | Modern container, H.264      |
| BMP, TIFF, ICO, GIF       | PNG            | Lossless compression         |
| JPEG                      | AVIF           | Best lossy image codec       |
| PDF                       | PDF (Optimized)| 150 PPI image downsampling   |

### Agent Mode

A background service that watches folders and auto-converts files matching your rules. Set up rules through the interactive menu, then the agent runs as a detached background process that survives terminal close.

**Trigger modes:**
- **Watch** — converts files as soon as they appear (Tokio-backed OS notifications)
- **Periodic** — scans folders at a configurable interval

Agend writes structured tracing logs to `~/.config/rusty-crunch/agent.log`.

## Project structure

```
rusty-crunch/
├── Cargo.toml
├── src/
│   ├── main.rs         # Entry point, main menu, recommended crunch
│   ├── agent.rs        # Agent Mode — tokio async watch service & background daemon
│   ├── config.rs       # Persistent settings + agent rules
│   ├── formats.rs      # Media types, format defs, compatibility map
│   ├── prompt.rs       # Interactive prompts (dialoguer)
│   ├── converter.rs    # Asynchronous I/O execution of external tools
│   ├── processor.rs    # Async batch processing (tokio) & DashMap Atomic Caching
│   ├── deps.rs         # Auto-detect PM & install missing tools
│   └── util.rs         # Shared utilities (cores, has, H.264 encoder detection)
```

## Acknowledgments

rusty-crunch leverages an incredible ecosystem of open-source Rust crates:

- **[tokio](https://crates.io/crates/tokio)** — Asynchronous runtime for fast I/O
- **[dashmap](https://crates.io/crates/dashmap)** — Blazing fast concurrent caching
- **[tracing](https://crates.io/crates/tracing)** — Application-level logging and diagnostics
- **[notify](https://crates.io/crates/notify)** — File system watching (Agent Mode)
- **[clap](https://crates.io/crates/clap)** & **[dialoguer](https://crates.io/crates/dialoguer)** — CLI mapping and interactive UI

## License

MIT — see [LICENSE](LICENSE).
