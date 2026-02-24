# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] — 2026-02-24

### Added

- **Recommended Crunch Mode** — new main menu option that automatically converts all media in a folder to the most efficient format:
  - Audio: WAV → FLAC (lossless), MP3/OGG/AAC/M4A/WMA → OPUS (lossy)
  - Video: AVI/MOV/FLV/WMV/TS → MKV
  - Images: BMP/TIFF/ICO/GIF → PNG (lossless), JPEG → AVIF (lossy)
  - Documents: PDF → PDF (Optimized with 150 PPI downsampling)
- **Display Mode Settings** — Verbose (show everything) or Clean (clear screen between interactions)
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
- Cross-platform: Windows-safe ImageMagick (`magick` not `convert`), Ghostscript (`gswin64c`), LibreOffice (`soffice` fallback)
- UNC path prefix stripping on Windows

## [0.2.0] — 2026-02-17

### Added
- GitHub Actions CI workflow for cross-platform releases
- Improved README with installation instructions

## [0.1.0] — 2026-02-10

### Added
- Initial release with parallel media conversion
