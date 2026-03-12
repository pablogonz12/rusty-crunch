# Changelog

All notable changes to this project will be documented in this file.

## [0.5.1] — 2026-03-12

### Fixed

- **PDF re-optimization skip** — same-extension optimizations (PDF → PDF) now cache file metadata (size + mtime) in `~/.config/rusty-crunch/opt_cache.json` so already-optimized files are instantly skipped on repeat runs
- Added 2 new unit tests for the optimization cache (`cache_entry_matches_unchanged_file`, `cache_roundtrip_serialize`)

## [0.5.0] — 2026-03-10

### Added

- **Agent Mode [BETA]** — automated background file conversion service
  - Configure rules: folder + input format → output format with recursive/delete options
  - **Watch trigger** — converts files as soon as they appear (via OS file system notifications)
  - **Periodic trigger** — scans folders at a configurable interval
  - Runs as a **true background service** that survives terminal close
  - PID file management for clean start/stop lifecycle
  - Log output to `~/.config/rusty-crunch/agent.log`
- New CLI flags: `--agent`, `--agent-stop`, `--agent-status`
- Platform-aware BETA/ALPHA labeling (ALPHA on macOS due to limited testing)
- Non-interactive dependency checking for headless agent mode (`deps::check()`)
- Shared `human_bytes()` utility in `util.rs`

### Fixed

- Agent gracefully skips non-existent watch folders instead of crashing
- Periodic mode enforces minimum 60-second interval
- Clean PID file removal on early exit
- Stale PID detection and cleanup

### Changed

- Agent spawns as detached background process (Unix: `setsid()`, Windows: `DETACHED_PROCESS`)
- SIGTERM handler on Unix for clean shutdown when killed
- Version bumped to 0.5.0

### Dependencies

- Added: notify 6 (file system watching), ctrlc 3 (signal handling), libc 0.2 (Unix process management)

## [0.4.0] — 2026-02-24

### Added

- **Recommended Crunch Mode** — new main menu option that automatically converts all media in a folder to the most efficient format:
  - Audio: WAV → FLAC (lossless), MP3/OGG/AAC/M4A/WMA → OPUS (lossy)
  - Video: AVI/MOV/FLV/WMV/TS → MKV
  - Images: BMP/TIFF/ICO/GIF → PNG (lossless), JPEG → AVIF (lossy)
  - Documents: PDF → PDF (Optimized with 150 PPI downsampling)
- **Display Mode Settings** — Verbose (show everything) or Clean (clear screen between interactions)
- **GitHub Actions CI** — automated cross-platform release builds (Linux, macOS, Windows)
- Display mode selector in Settings → Edit defaults

### Fixed

- PDF → PDF conversion now properly differs from PDF (Optimized) — prevents same-extension skip bug
- JPEG output extension correctly maps to `.jpg` (not `.jpeg`)
- H.264 encoder detection via runtime frame probe (detects h264_nvenc, h264_qsv, libx264, libopenh264)
- LibreOffice thread-safety via global mutex
- `delete_originals` setting now defaults to `false`
- Error cleanup safely skips in-place optimizations (same-extension conversions)

### Technical

- Refactored `converter.rs` to dispatch via MediaType enum
- Created `util.rs` for shared utilities (cores, has(), best_h264_encoder)
- Config struct serializes to JSON in `~/.config/rusty-crunch/config.json`
- Parallel processing via rayon with CAS-based atomic compression ratio tracking
- Progress bar with ETA, best/worst compression stats

### Dependencies

- Added: clap 4 (CLI parsing with derive macros)
- Video: ffmpeg with hardware encoder probing
- Images: ImageMagick (magick/convert)
- Documents: Ghostscript (PDF optimization), LibreOffice (format conversion)
- Config: serde, serde_json (JSON serialization)
- UI: dialoguer 0.11 (interactive prompts), console 0.15 (colors/terminal control)
- Parallel: rayon 1.10, indicatif 0.17 (progress bars)
- Utils: walkdir 2.5 (directory traversal), dirs 5.0 (config path)

---

## [0.3.0] — Earlier

(See git history for details)
