use anyhow::{bail, Result};
#[cfg(target_os = "windows")]
use anyhow::Context;
use console::style;
use serde_json::Value;
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
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

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
        cache.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Total threads to use for parallel processing, based on the configured thread mode.
pub fn active_threads() -> usize {
    let total = cores();
    crate::config::load().thread_mode.to_threads(total)
}

// ── Size parsing ───────────────────────────────────────────────────────────────────────

/// Parse a size string like "10MB", "1.5GB", "512KB" into bytes.
/// Returns None if parsing fails.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    let (num_str, unit) = if let Some(pos) = s.find(|c: char| c.is_alphabetic()) {
        s.split_at(pos)
    } else {
        return None;
    };

    let num: f64 = num_str.trim().parse().ok()?;
    if num < 0.0 {
        return None;
    }

    let multiplier: u64 = match unit.trim() {
        "B" => 1,
        "KB" => 1_024,
        "MB" => 1_024 * 1_024,
        "GB" => 1_024 * 1_024 * 1_024,
        "TB" => 1_024u64 * 1_024 * 1_024 * 1_024,
        _ => return None,
    };

    Some((num * multiplier as f64) as u64)
}

// ── Auto-update ───────────────────────────────────────────────────────────────────────

/// Check GitHub for a newer release of rusty-crunch.
/// Returns `Ok(Some(version))` if a newer version is available,
/// `Ok(None)` if already up-to-date, or `Err` if the check failed.
pub fn check_for_update() -> Result<Option<String>> {
    if !has("curl") {
        bail!("curl is not installed — required for update checks");
    }
    let output = Command::new("curl")
        .args(["-sf", "--connect-timeout", "5", "--max-time", "10",
               "https://api.github.com/repos/pablogonz12/rusty-crunch/releases/latest"])
        .stderr(Stdio::null())
        .output()
        .map_err(|e| anyhow::anyhow!("curl failed: {}", e))?;

    if !output.status.success() {
        bail!("Could not reach GitHub — check your internet connection");
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&body)
        .map_err(|_| anyhow::anyhow!("Unexpected response from GitHub API"))?;
    let tag = json["tag_name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("tag_name missing from GitHub response"))?;
    let ver = tag.trim_start_matches('v').to_string();
    if is_newer_than_current(&ver) { Ok(Some(ver)) } else { Ok(None) }
}

fn is_newer_than_current(ver: &str) -> bool {
    fn parse(v: &str) -> [u32; 3] {
        let mut p = v.splitn(3, '.');
        [
            p.next().and_then(|x| x.parse().ok()).unwrap_or(0),
            p.next().and_then(|x| x.parse().ok()).unwrap_or(0),
            p.next().and_then(|x| x.parse().ok()).unwrap_or(0),
        ]
    }
    parse(ver) > parse(env!("CARGO_PKG_VERSION"))
}

fn update_artifact() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("rusty-crunch-linux-x86_64")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("rusty-crunch-macos-aarch64")
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some("rusty-crunch-macos-x86_64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("rusty-crunch-windows-x86_64.exe")
    } else {
        None
    }
}

/// Download and install the given version. Replaces the running binary and restarts.
/// On Windows: swaps via a helper batch script.
/// On Unix: atomic rename, then exec the new binary in-place.
pub fn download_and_install_update(version: &str) -> Result<()> {
    let artifact = update_artifact()
        .ok_or_else(|| anyhow::anyhow!("Auto-update not supported on this platform"))?;

    let url = format!(
        "https://github.com/pablogonz12/rusty-crunch/releases/download/v{version}/{artifact}"
    );

    let current_exe = std::env::current_exe()?;
    let tmp = current_exe.with_extension("update_tmp");

    println!("  {} Downloading v{} \u{2026}", style("\u{2b07}").cyan(), style(version).white().bold());

    let status = Command::new("curl")
        .args(["-L", "--connect-timeout", "30", "--max-time", "300", "-#", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status()
        .map_err(|_| anyhow::anyhow!("curl not found — install it to use auto-update"))?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("Download failed — check your internet connection");
    }

    #[cfg(target_os = "windows")]
    {
        let new_exe = current_exe.with_extension("new.exe");
        std::fs::rename(&tmp, &new_exe)?;

        let cur = current_exe.to_string_lossy().replace('"', "\"\"");
        let new = new_exe.to_string_lossy().replace('"', "\"\"");
        let bat = current_exe.with_extension("upd.bat");
        let bat_str = bat
            .to_str()
            .context("Update batch path is not valid UTF-8")?;
        std::fs::write(
            &bat,
            format!("@echo off\r\nping -n 3 127.0.0.1>nul\r\nmove /y \"{new}\" \"{cur}\"\r\nstart \"\" \"{cur}\"\r\ndel \"%~f0\"\r\n").as_bytes(),
        )?;
        Command::new("cmd")
            .args(["/C", "start", "/min", "", bat_str])
            .spawn()?;
        println!("\n  {} Updated to v{}. Restarting\u{2026}\n", style("\u{2714}").green().bold(), version);
        std::process::exit(0);
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::CommandExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &current_exe)?;
        println!("\n  {} Updated to v{}. Restarting\u{2026}\n", style("\u{2714}").green().bold(), version);
        let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        let err = Command::new(&current_exe).args(&args).exec();
        bail!("Restart failed: {}", err);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size_bytes() {
        assert_eq!(parse_size("100"), None); // no unit
        assert_eq!(parse_size("100B"), Some(100));
        assert_eq!(parse_size("0B"), Some(0));
    }

    #[test]
    fn test_parse_size_kilobytes() {
        assert_eq!(parse_size("1KB"), Some(1024));
        assert_eq!(parse_size("10KB"), Some(10240));
        assert_eq!(parse_size("1.5KB"), Some(1536));
    }

    #[test]
    fn test_parse_size_megabytes() {
        assert_eq!(parse_size("1MB"), Some(1048576));
        assert_eq!(parse_size("10MB"), Some(10485760));
        assert_eq!(parse_size("0.5MB"), Some(524288));
    }

    #[test]
    fn test_parse_size_gigabytes() {
        assert_eq!(parse_size("1GB"), Some(1073741824));
        assert_eq!(parse_size("2GB"), Some(2147483648));
    }

    #[test]
    fn test_parse_size_terabytes() {
        assert_eq!(parse_size("1TB"), Some(1099511627776));
    }

    #[test]
    fn test_parse_size_case_insensitive() {
        assert_eq!(parse_size("10mb"), Some(10485760));
        assert_eq!(parse_size("10MB"), Some(10485760));
        assert_eq!(parse_size("10Mb"), Some(10485760));
    }

    #[test]
    fn test_parse_size_with_spaces() {
        assert_eq!(parse_size("  10 MB  "), Some(10485760));
        assert_eq!(parse_size("10   MB"), Some(10485760));
    }

    #[test]
    fn test_parse_size_invalid() {
        assert_eq!(parse_size("10XB"), None); // invalid unit
        assert_eq!(parse_size("-10MB"), None); // negative
        assert_eq!(parse_size("abc"), None); // not a number
        assert_eq!(parse_size(""), None); // empty
    }

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(100), "100 B");
        assert_eq!(human_bytes(1024), "1 KB");
        assert_eq!(human_bytes(1048576), "1.0 MB");
        assert_eq!(human_bytes(1073741824), "1.00 GB");
    }

    #[test]
    fn test_human_bytes_edge_cases() {
        // Test boundary conditions
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1 KB");
        assert_eq!(human_bytes(1025), "1 KB");
        assert_eq!(human_bytes(2048), "2 KB");
        assert_eq!(human_bytes(2560), "2 KB");      // 2.5 KB rounds to 2 with {:.0}
        assert_eq!(human_bytes(1047552), "1023 KB");  // 1047552 / 1024 = 1022.99... ≈ 1023
        assert_eq!(human_bytes(1048576), "1.0 MB");   // Exactly 1 MB
        assert_eq!(human_bytes(1572864), "1.5 MB");
        assert_eq!(human_bytes(1073741823), "1024.0 MB"); // Just under 1 GB
        assert_eq!(human_bytes(1073741824), "1.00 GB");   // Exactly 1 GB
        assert_eq!(human_bytes(2147483648), "2.00 GB");   // 2 GB
    }

    #[test]
    fn test_parse_size_decimal_edge_cases() {
        // Decimal parsing with various levels of precision
        assert_eq!(parse_size("0.5B"), Some(0));      // Rounds down
        assert_eq!(parse_size("0.5KB"), Some(512));   // Float calc: 0.5 * 1024 = 512
        assert_eq!(parse_size("2.5KB"), Some(2560));  // 2.5 * 1024 = 2560
        assert_eq!(parse_size("1.25MB"), Some(1310720)); // 1.25 * 1024 * 1024 = 1310720
        assert_eq!(parse_size("0.1MB"), Some(104857)); // 0.1 * 1024 * 1024 ≈ 104857.6, truncated to 104857
        // Note: "99.99MB" produces approximately 104847114 due to floating point precision
    }

    #[test]
    fn test_parse_size_large_values() {
        // Test large file sizes
        assert_eq!(parse_size("1TB"), Some(1099511627776));
        assert_eq!(parse_size("100TB"), Some(109951162777600));
        assert_eq!(parse_size("1.5TB"), Some(1649267441664));
    }

    #[test]
    fn test_parse_size_whitespace_handling() {
        // Various whitespace combinations
        assert_eq!(parse_size("10MB"), Some(10485760));
        assert_eq!(parse_size("   10MB   "), Some(10485760));
        assert_eq!(parse_size("10   MB"), Some(10485760));
        assert_eq!(parse_size("  10  MB  "), Some(10485760));
        assert_eq!(parse_size("	10MB"), Some(10485760)); // Tab
    }

    #[test]
    fn test_parse_size_invalid_edge_cases() {
        // More invalid cases
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("   "), None);
        assert_eq!(parse_size("MB"), None);        // No number
        assert_eq!(parse_size("10"), None);        // No unit
        assert_eq!(parse_size("10 10 MB"), None);  // Double number
        assert_eq!(parse_size("10.5.5MB"), None);  // Multiple decimals
        assert_eq!(parse_size("10XB"), None);      // Invalid unit
        assert_eq!(parse_size("abc MB"), None);    // Non-numeric
        assert_eq!(parse_size("--10MB"), None);    // Double negative
    }

    #[test]
    fn test_cores() {
        // cores() should return at least 1
        assert!(cores() >= 1);
        // Should match available_parallelism (or 1 if unavailable)
        assert_eq!(cores(), std::thread::available_parallelism().map_or(1, |n| n.get()));
    }

    #[test]
    fn test_has_caching() {
        // Clear cache first
        clear_has_cache();
        // Common tools should be detectable or not consistently
        let result = has("true"); // 'true' is a standard POSIX utility
        assert!(result); // Should exist on all POSIX systems
        
        // Call again - should use cache
        let result2 = has("true");
        assert_eq!(result, result2);
    }

    #[test]
    fn test_active_threads() {
        // Should return a reasonable number of threads
        let threads = active_threads();
        assert!(threads >= 1);
        assert!(threads <= cores() * 2); // Sanity check
    }
}
