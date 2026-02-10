use crate::converter;
use crate::formats::MediaType;
use crate::util;
use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use walkdir::WalkDir;

pub struct Job<'a> {
    pub folder: &'a Path,
    pub media_type: MediaType,
    pub input_fmt: &'a str,
    pub output_fmt: &'a str,
    pub recursive: bool,
    pub delete_originals: bool,
    pub dry_run: bool,
}

pub fn run(job: &Job) -> Result<()> {
    let input_ext = job.input_fmt.to_ascii_lowercase();
    let output_ext = match job.output_fmt {
        "PDF (Optimized)" => "pdf".to_string(),
        "JPEG" => "jpg".to_string(),
        other => other.to_ascii_lowercase(),
    };
    let same_ext = input_ext == output_ext;

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
    let cores = util::cores();

    // ── Dry-run mode ────────────────────────────────────────────────
    if job.dry_run {
        let total_bytes: u64 = files.iter()
            .filter_map(|f| f.metadata().ok())
            .map(|m| m.len())
            .sum();
        println!(
            "\n  {} Dry run: would convert {} file{} ({} total input size)",
            style("🔍").cyan(),
            style(total).cyan().bold(),
            if total == 1 { "" } else { "s" },
            style(util::human_bytes(total_bytes)).white().bold(),
        );
        return Ok(());
    }

    println!(
        "\n  {} Found {} file{} · using {} thread{}",
        style("⚡").cyan(),
        style(total).cyan().bold(),
        if total == 1 { "" } else { "s" },
        style(cores).cyan().bold(),
        if cores == 1 { "" } else { "s" },
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

    let ok_count = AtomicUsize::new(0);
    let skip_count = AtomicUsize::new(0);
    let err_count = AtomicUsize::new(0);
    let saved_bytes = AtomicU64::new(0);
    // Track best/worst compression ratios
    let best_ratio = AtomicU64::new(0);     // stored as ratio * 10000 (fixed point)
    let worst_ratio = AtomicU64::new(10000); // 100% = no savings (worst possible)
    let start = Instant::now();

    // ── Parallel processing via rayon ───────────────────────────────
    files.par_iter().for_each(|input_path| {
        let output_path = input_path.with_extension(&output_ext);

        // Skip if output already exists (not for in-place optimization)
        if !same_ext && output_path.exists() {
            skip_count.fetch_add(1, Ordering::Relaxed);
            pb.inc(1);
            return;
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
                    let _ = std::fs::remove_file(input_path);
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

    pb.finish_and_clear();

    // ── Summary ─────────────────────────────────────────────────────
    let elapsed = start.elapsed();
    let ok = ok_count.load(Ordering::Relaxed);
    let skipped = skip_count.load(Ordering::Relaxed);
    let errs = err_count.load(Ordering::Relaxed);
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
    }
    if ok > 1 {
        println!(
            "  {} Compression   best: {:.1}%   worst: {:.1}%",
            style("┃").dim(),
            best as f64 / 100.0,
            worst as f64 / 100.0,
        );
    }
    println!(
        "  {} Finished in {:.1}s on {} cores",
        style("┃").dim(),
        elapsed.as_secs_f64(),
        cores,
    );
    println!("  {}", style("─".repeat(50)).dim());

    Ok(())
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
