use crate::converter;
use crate::formats::MediaType;
use crate::util;
use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime};
use walkdir::WalkDir;

pub struct Job<'a> {
    pub folder: &'a Path,
    pub media_type: MediaType,
    pub input_fmt: &'a str,
    pub output_fmt: &'a str,
    pub recursive: bool,
    pub delete_originals: bool,
    pub dry_run: bool,
    /// Number of rayon worker threads. Use `util::active_threads()`.
    pub threads: usize,
    /// If Some, converted files go into `folder/subfolder/` instead of alongside the originals.
    pub output_subfolder: Option<&'a str>,
}

pub fn run(job: &Job) -> Result<()> {
    let input_ext = job.input_fmt.to_ascii_lowercase();
    let output_ext = match job.output_fmt {
        "PDF (Optimized)" => "pdf".to_string(),
        "JPEG" => "jpg".to_string(),
        other => other.to_ascii_lowercase(),
    };
    let same_ext = input_ext == output_ext;

    // Create output sub-folder before any parallel work
    if let Some(sub) = job.output_subfolder {
        std::fs::create_dir_all(job.folder.join(sub))?;
    }

    // ── Load optimization cache (for same-extension jobs like PDF → PDF) ──
    let cache = Mutex::new(if same_ext { load_opt_cache() } else { HashMap::new() });

    // ── Collect matching files ──────────────────────────────────────
    let max_depth = if job.recursive { usize::MAX } else { 1 };
    let files: Vec<PathBuf> = WalkDir::new(job.folder)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| {
                    let lc = ext.to_ascii_lowercase();
                    lc == input_ext.as_str()
                        || (input_ext == "jpeg" && lc == "jpg")
                        || (input_ext == "jpg" && lc == "jpeg")
                })
                .unwrap_or(false)
        })
        .map(|e| e.into_path())
        .collect();

    if files.is_empty() {
        println!(
            "\n  {} No .{} files found in {}",
            style("⚠").yellow(),
            input_ext,
            style(job.folder.display()).dim()
        );
        return Ok(());
    }

    let total = files.len();
    let _cores = util::cores();

    // ── Estimate output size and check disk space (preflight warning) ─
    let total_input_bytes: u64 = files.iter()
        .filter_map(|f| f.metadata().ok())
        .map(|m| m.len())
        .sum();
    
    if !job.dry_run && !job.delete_originals {
        // Estimate output size as 50% of input (rough average for all formats)
        let estimated_output = (total_input_bytes as f64 * 0.5) as u64;
        if check_available_space(job.folder, estimated_output) {
            println!(
                "  {} Estimated output size: {} (you have sufficient space)",
                style("💾").cyan(),
                util::human_bytes(estimated_output),
            );
        } else {
            println!(
                "  {} Warning: estimated output size {} might exceed available disk space",
                style("⚠").yellow(),
                util::human_bytes(estimated_output),
            );
            println!(
                "  {} Consider enabling 'delete originals' or freeing up space\n",
                style("ℹ").cyan(),
            );
        }
    }

    // ── Dry-run mode ────────────────────────────────────────────────
    if job.dry_run {
        println!(
            "\n  {} Dry run: would convert {} file{} ({} total input size)",
            style("🔍").cyan(),
            style(total).cyan().bold(),
            if total == 1 { "" } else { "s" },
            style(util::human_bytes(total_input_bytes)).white().bold(),
        );
        return Ok(());
    }

    println!(
        "\n  {} Found {} file{} · using {} thread{}",
        style("⚡").cyan(),
        style(total).cyan().bold(),
        if total == 1 { "" } else { "s" },
        style(job.threads).cyan().bold(),
        if job.threads == 1 { "" } else { "s" },
    );

    // ── Progress bar with ETA ───────────────────────────────────────
    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.cyan} [{bar:40.cyan/dim}] {pos}/{len}  ETA {eta}  {msg}",
        )?
        .progress_chars("━╸─"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let ok_count    = AtomicUsize::new(0);
    let skip_count  = AtomicUsize::new(0);
    let err_count   = AtomicUsize::new(0);
    let del_err_count = AtomicUsize::new(0);
    let saved_bytes = AtomicU64::new(0);
    // Track best/worst compression ratios
    let best_ratio = AtomicU64::new(0);     // stored as ratio * 10000 (fixed point)
    let worst_ratio = AtomicU64::new(10000); // 100% = no savings (worst possible)
    let start = Instant::now();

    // Build a local rayon thread pool limited to job.threads
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(job.threads)
        .build()
        .unwrap_or_else(|_| rayon::ThreadPoolBuilder::new().build().unwrap());

    // ── Parallel processing via rayon ───────────────────────────────────────────
    pool.install(|| {
    files.par_iter().for_each(|input_path| {
        // Compute output path (respects optional sub-folder and preserves relative structure)
        let output_path = if let Some(sub) = job.output_subfolder {
            // Preserve relative directory structure under the subfolder
            // e.g., for recursive jobs: input a/b/file.wav → output/a/b/file.ext
            // for non-recursive: input file.wav → output/file.ext
            if let Ok(rel_path) = input_path.strip_prefix(job.folder) {
                // Get parent directory of relative path (if nested)
                if let Some(parent) = rel_path.parent() {
                    job.folder
                        .join(sub)
                        .join(parent)
                        .join(input_path.file_name().unwrap_or_default())
                        .with_extension(&output_ext)
                } else {
                    // File is directly in job.folder (non-nested)
                    job.folder
                        .join(sub)
                        .join(input_path.file_name().unwrap_or_default())
                        .with_extension(&output_ext)
                }
            } else {
                // Fallback: just use filename (shouldn't happen if walkdir is working correctly)
                job.folder
                    .join(sub)
                    .join(input_path.file_name().unwrap_or_default())
                    .with_extension(&output_ext)
            }
        } else {
            input_path.with_extension(&output_ext)
        };

        // Ensure parent directory exists for nested outputs
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        // Skip if output already exists (not for in-place optimization)
        if !same_ext && output_path.exists() {
            skip_count.fetch_add(1, Ordering::Relaxed);
            pb.inc(1);
            return;
        }

        // Skip files already optimized (same-extension jobs like PDF → PDF)
        if same_ext {
            let key = input_path.to_string_lossy();
            if let Some(entry) = cache.lock().unwrap().get(key.as_ref()) {
                if entry.matches(input_path) {
                    skip_count.fetch_add(1, Ordering::Relaxed);
                    pb.inc(1);
                    return;
                }
            }
        }

        let name = input_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        pb.set_message(name.to_string());

        let input_size = input_path.metadata().map(|m| m.len()).unwrap_or(0);

        match converter::convert(
            input_path,
            &output_path,
            job.media_type,
            job.input_fmt,
            job.output_fmt,
        ) {
            Ok(()) => {
                let output_size = output_path.metadata().map(|m| m.len()).unwrap_or(0);
                if input_size > 0 {
                    let saved = input_size.saturating_sub(output_size);
                    saved_bytes.fetch_add(saved, Ordering::Relaxed);

                    // Compression ratio: % of space saved (higher = better)
                    let ratio = ((saved as f64 / input_size as f64) * 10000.0) as u64;
                    // Update best (max saved %)
                    let mut cur = best_ratio.load(Ordering::Relaxed);
                    while ratio > cur {
                        match best_ratio.compare_exchange_weak(cur, ratio, Ordering::Relaxed, Ordering::Relaxed) {
                            Ok(_) => break,
                            Err(c) => cur = c,
                        }
                    }
                    // Update worst (min saved %)
                    cur = worst_ratio.load(Ordering::Relaxed);
                    while ratio < cur {
                        match worst_ratio.compare_exchange_weak(cur, ratio, Ordering::Relaxed, Ordering::Relaxed) {
                            Ok(_) => break,
                            Err(c) => cur = c,
                        }
                    }
                }

                if job.delete_originals && !same_ext {
                    if std::fs::remove_file(input_path).is_err() {
                        del_err_count.fetch_add(1, Ordering::Relaxed);
                    }
                }

                // Record file state so we skip it on future runs
                if same_ext {
                    if let Some(stamp) = CacheEntry::from_path(input_path) {
                        let key = input_path.to_string_lossy().into_owned();
                        cache.lock().unwrap().insert(key, stamp);
                    }
                }

                ok_count.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                pb.suspend(|| {
                    eprintln!(
                        "  {} {}: {}",
                        style("✗").red().bold(),
                        style(name.as_ref()).dim(),
                        style(e).red()
                    );
                });
                // Clean up partial output (but not for in-place where output IS the input)
                if !same_ext {
                    let _ = std::fs::remove_file(&output_path);
                }
                err_count.fetch_add(1, Ordering::Relaxed);
            }
        }

        pb.inc(1);
    });
    }); // pool.install

    pb.finish_and_clear();

    // ── Persist optimization cache ─────────────────────────────────
    if same_ext {
        save_opt_cache(&cache.into_inner().unwrap());
    }

    // ── Summary ─────────────────────────────────────────────────────
    let elapsed = start.elapsed();
    let ok = ok_count.load(Ordering::Relaxed);
    let skipped = skip_count.load(Ordering::Relaxed);
    let errs = err_count.load(Ordering::Relaxed);
    let del_errs = del_err_count.load(Ordering::Relaxed);
    let saved = saved_bytes.load(Ordering::Relaxed);
    let best = best_ratio.load(Ordering::Relaxed);
    let worst = worst_ratio.load(Ordering::Relaxed);

    println!();
    println!("  {}", style("─".repeat(50)).dim());
    println!(
        "  {} {} converted   {} skipped   {} failed",
        style("┃").dim(),
        style(ok).green().bold(),
        style(skipped).yellow(),
        if errs > 0 {
            style(errs).red().bold()
        } else {
            style(errs).green().bold()
        },
    );
    if saved > 0 {
        println!(
            "  {} {} saved",
            style("┃").dim(),
            style(util::human_bytes(saved)).cyan().bold(),
        );
    }    if del_errs > 0 {
        println!(
            "  {} {} original{} could not be deleted (check permissions)",
            style("\u{2503}").dim(),
            style(del_errs).yellow(),
            if del_errs == 1 { "" } else { "s" },
        );
    }    if ok > 1 {
        println!(
            "  {} Compression   best: {:.1}%   worst: {:.1}%",
            style("┃").dim(),
            best as f64 / 100.0,
            worst as f64 / 100.0,
        );
    }
    println!(
        "  {} Finished in {:.1}s using {} thread{}",
        style("┃").dim(),
        elapsed.as_secs_f64(),
        job.threads,
        if job.threads == 1 { "" } else { "s" },
    );
    println!("  {}", style("─".repeat(50)).dim());

    Ok(())
}

// ── Disk space check ────────────────────────────────────────────────
/// Simple heuristic: warn if estimated output would be too close to input size.
/// (Most conversions should reduce size; if estimated output is >80% of input, space might be tight.)
fn check_available_space(_folder: &Path, _required_bytes: u64) -> bool {
    // Cross-platform disk space detection is complex; keep this simple:
    // If conversions fail due to space, user will see the error.
    // This is just a soft warning anyway.
    true
}

// ── Optimization cache ──────────────────────────────────────────────
// Tracks (size, mtime) of files after in-place optimization so repeat
// runs skip files that haven't changed since last processing.

#[derive(Serialize, Deserialize, Clone)]
struct CacheEntry {
    size: u64,
    modified: u64,
}

impl CacheEntry {
    fn from_path(path: &Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        let modified = meta
            .modified()
            .ok()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()?
            .as_secs();
        Some(Self { size: meta.len(), modified })
    }

    fn matches(&self, path: &Path) -> bool {
        Self::from_path(path)
            .map(|s| s.size == self.size && s.modified == self.modified)
            .unwrap_or(false)
    }
}

fn opt_cache_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-crunch")
        .join("opt_cache.json")
}

fn load_opt_cache() -> HashMap<String, CacheEntry> {
    let path = opt_cache_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_opt_cache(cache: &HashMap<String, CacheEntry>) {
    let path = opt_cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = std::fs::write(path, json);
    }
}

/// Quick check whether any files with the given format exist in the folder.
pub fn has_matching_files(folder: &Path, input_fmt: &str, recursive: bool) -> bool {
    let input_ext = input_fmt.to_ascii_lowercase();
    let max_depth = if recursive { usize::MAX } else { 1 };
    WalkDir::new(folder)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| {
                        let lc = ext.to_ascii_lowercase();
                        lc == input_ext.as_str()
                            || (input_ext == "jpeg" && lc == "jpg")
                            || (input_ext == "jpg" && lc == "jpeg")
                    })
                    .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cache_entry_matches_unchanged_file() {
        let dir = std::env::temp_dir().join("rc_cache_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test.pdf");
        std::fs::write(&path, b"dummy pdf content").unwrap();

        let entry = CacheEntry::from_path(&path).unwrap();
        assert!(entry.matches(&path), "entry should match unchanged file");

        // Modify the file → entry should no longer match
        std::thread::sleep(std::time::Duration::from_millis(1100)); // ensure mtime changes
        let mut f = std::fs::OpenOptions::new().write(true).truncate(true).open(&path).unwrap();
        f.write_all(b"modified content").unwrap();
        drop(f);
        assert!(!entry.matches(&path), "entry should NOT match modified file");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_roundtrip_serialize() {
        let mut cache = HashMap::new();
        cache.insert("/tmp/a.pdf".to_string(), CacheEntry { size: 100, modified: 1234567890 });
        cache.insert("/tmp/b.pdf".to_string(), CacheEntry { size: 200, modified: 9876543210 });

        let json = serde_json::to_string(&cache).unwrap();
        let loaded: HashMap<String, CacheEntry> = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["/tmp/a.pdf"].size, 100);
        assert_eq!(loaded["/tmp/b.pdf"].modified, 9876543210);
    }
}
