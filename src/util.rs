use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

/// Number of available CPU cores (cached).
pub fn cores() -> usize {
    static CORES: OnceLock<usize> = OnceLock::new();
    *CORES.get_or_init(|| {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
    })
}

/// Module-level cache for `has()` lookups.
static HAS_CACHE: OnceLock<Mutex<Vec<(String, bool)>>> = OnceLock::new();

/// Returns true if `name` is found in PATH (cached per unique name).
pub fn has(name: &str) -> bool {
    let cache = HAS_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = cache.lock().unwrap();

    if let Some(entry) = guard.iter().find(|(n, _)| n == name) {
        return entry.1;
    }

    let found = has_uncached(name);
    guard.push((name.to_string(), found));
    found
}

/// Clear the has() cache so freshly installed tools are detected.
pub fn clear_has_cache() {
    if let Some(cache) = HAS_CACHE.get() {
        cache.lock().unwrap().clear();
    }
}

fn has_uncached(name: &str) -> bool {
    #[cfg(target_os = "windows")]
    let checker = "where";
    #[cfg(not(target_os = "windows"))]
    let checker = "which";

    Command::new(checker)
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Detect the best H.264 encoder available in ffmpeg (cached).
/// Returns (encoder, quality_args) — e.g. ("libx264", &["-preset","medium","-crf","23"])
/// or ("h264_nvenc", &["-preset","p4","-crf","23"]) for NVIDIA GPUs.
pub fn best_h264_encoder() -> &'static H264Encoder {
    static ENCODER: OnceLock<H264Encoder> = OnceLock::new();
    ENCODER.get_or_init(detect_h264_encoder)
}

pub struct H264Encoder {
    pub name: &'static str,
    pub quality_args: &'static [&'static str],
}

fn detect_h264_encoder() -> H264Encoder {
    // Try encoders in preference order by actually probing ffmpeg.
    // Just checking `-encoders` output is insufficient — an encoder may be
    // listed but fail at runtime (e.g. h264_nvenc without CUDA drivers).

    #[cfg(target_os = "macos")]
    if probe_encoder("h264_videotoolbox") {
        return H264Encoder {
            name: "h264_videotoolbox",
            quality_args: &["-q:v", "65"],
        };
    }

    #[cfg(not(target_os = "macos"))]
    {
        if probe_encoder("h264_nvenc") {
            return H264Encoder {
                name: "h264_nvenc",
                quality_args: &["-preset", "p4", "-crf", "23"],
            };
        }

        if probe_encoder("h264_qsv") {
            return H264Encoder {
                name: "h264_qsv",
                quality_args: &["-global_quality", "23"],
            };
        }
    }

    if probe_encoder("libx264") {
        return H264Encoder {
            name: "libx264",
            quality_args: &["-preset", "medium", "-crf", "23"],
        };
    }

    // Last resort — libopenh264 (Fedora/RHEL ship this instead of libx264)
    H264Encoder {
        name: "libopenh264",
        quality_args: &[],
    }
}

/// Resolve the Ghostscript binary name for the current platform.
/// On Windows, Ghostscript installs as `gswin64c` / `gswin32c`, not `gs`.
pub fn gs_command() -> &'static str {
    static CMD: OnceLock<&str> = OnceLock::new();
    *CMD.get_or_init(|| {
        if has("gs") { return "gs"; }
        #[cfg(target_os = "windows")]
        {
            if has("gswin64c") { return "gswin64c"; }
            if has("gswin32c") { return "gswin32c"; }
        }
        "gs"
    })
}

/// Resolve the LibreOffice binary name.
/// On Windows / some macOS installs the command is `soffice`, not `libreoffice`.
pub fn lo_command() -> &'static str {
    static CMD: OnceLock<&str> = OnceLock::new();
    *CMD.get_or_init(|| {
        if has("libreoffice") { return "libreoffice"; }
        if has("soffice") { return "soffice"; }
        "libreoffice"
    })
}

/// Resolve the ImageMagick binary name.
/// On Windows, NEVER fall back to `convert` — that is a built-in disk utility.
pub fn magick_command() -> &'static str {
    static CMD: OnceLock<&str> = OnceLock::new();
    *CMD.get_or_init(|| {
        if has("magick") { return "magick"; }
        #[cfg(not(target_os = "windows"))]
        if has("convert") { return "convert"; }
        "magick"
    })
}

/// Returns true if Ghostscript is available (any platform-specific binary name).
pub fn has_gs() -> bool {
    has("gs") || has("gswin64c") || has("gswin32c")
}

/// Returns true if LibreOffice is available (any known binary name).
pub fn has_lo() -> bool {
    has("libreoffice") || has("soffice")
}

/// Returns true if ImageMagick is available.
/// On Windows, only checks for `magick` (never `convert`).
pub fn has_magick() -> bool {
    if has("magick") { return true; }
    #[cfg(not(target_os = "windows"))]
    if has("convert") { return true; }
    false
}

/// Format byte count as a human-friendly string (e.g. "1.4 MB").
pub fn human_bytes(b: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if b >= GB {
        format!("{:.2} GB", b as f64 / GB as f64)
    } else if b >= MB {
        format!("{:.1} MB", b as f64 / MB as f64)
    } else if b >= KB {
        format!("{:.0} KB", b as f64 / KB as f64)
    } else {
        format!("{b} B")
    }
}

/// Try to actually initialize an encoder — returns true only if it can start.
fn probe_encoder(name: &str) -> bool {
    Command::new("ffmpeg")
        .args([
            "-hide_banner", "-loglevel", "error",
            "-f", "lavfi", "-i", "color=black:s=64x64:d=0.04:r=25",
            "-c:v", name, "-frames:v", "1",
            "-f", "null", "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
