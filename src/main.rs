mod agent;
mod config;
mod converter;
mod deps;
mod formats;
mod processor;
mod prompt;
mod util;

use anyhow::Result;
use clap::Parser;
use console::style;
use dialoguer::{Select, theme::ColorfulTheme};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rusty-crunch", version, about = "Fast parallel media converter")]
struct Cli {
    /// Simulate the run without converting anything
    #[arg(long)]
    dry_run: bool,

    /// Run the agent as a background service (reads rules from config)
    #[arg(long)]
    agent: bool,

    /// Stop a running background agent
    #[arg(long)]
    agent_stop: bool,

    /// Check if the background agent is currently running
    #[arg(long)]
    agent_status: bool,

    /// Check that all required tools are installed
    #[arg(long)]
    health_check: bool,

    /// Minimum file size (e.g., "10MB", "1GB"). Only process files larger than this.
    #[arg(long, value_name = "SIZE")]
    min_size: Option<String>,

    /// Maximum file size (e.g., "500MB", "2GB"). Only process files smaller than this.
    #[arg(long, value_name = "SIZE")]
    max_size: Option<String>,

    /// Folder to process (skips the directory browser)
    #[arg(value_name = "FOLDER")]
    folder: Option<PathBuf>,

    /// Print conversion summary as JSON to stdout
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.agent {
        return agent::run_headless();
    }
    if cli.agent_stop {
        return agent::stop_background();
    }
    if cli.agent_status {
        return agent::show_status();
    }
    if cli.health_check {
        return run_health_check();
    }

    loop {
        let display_mode = config::load().display_mode;
        maybe_clear(display_mode);
        banner();

        let agent_label = if cfg!(target_os = "macos") {
            "🤖 Agent Mode [ALPHA]"
        } else {
            "🤖 Agent Mode [BETA]"
        };

        let menu = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do?")
            .items(&[
                "🔧 Start Crunching",
                "🚀 Recommended Crunch",
                agent_label,
                "⚙️  Settings",
                "🔄 Check for Updates",
                "🚪 Exit",
            ])
            .default(0)
            .interact_opt()?;

        match menu {
            Some(0) => {
                run_crunch(&cli)?;
                pause_before_menu();
            }
            Some(1) => {
                run_recommended_crunch(&cli)?;
                pause_before_menu();
            }
            Some(2) => agent::setup()?,
            Some(3) => config::edit_settings()?,
            Some(4) => { check_for_updates()?; pause_before_menu(); }
            _ => {
                println!("  {} Bye!\n", style("👋").cyan());
                break;
            }
        }
    }
    Ok(())
}

fn run_crunch(cli: &Cli) -> Result<()> {
    let cfg = config::load();

    // Parse size filters
    let min_size = cli.min_size.as_deref().and_then(util::parse_size);
    let max_size = cli.max_size.as_deref().and_then(util::parse_size);

    if cli.min_size.is_some() && min_size.is_none() {
        anyhow::bail!("Invalid --min-size format. Use: 10MB, 1GB, 512KB, etc.");
    }
    if cli.max_size.is_some() && max_size.is_none() {
        anyhow::bail!("Invalid --max-size format. Use: 500MB, 2GB, 100MB, etc.");
    }
    
    // ── Shared settings (asked once for all jobs) ───────────────────
    let folder = if let Some(ref f) = cli.folder {
        let f = if f.is_relative() { std::env::current_dir()?.join(f) } else { f.clone() };
        if !f.is_dir() { anyhow::bail!("Not a directory: {}", f.display()); }
        ack("Folder", &f.display().to_string());
        f
    } else {
        match prompt::select_folder(&cfg)? {
            Some(f) => { ack("Folder", &f.display().to_string()); f }
            None => return Ok(()),
        }
    };

    let recursive = match prompt::confirm_scan_subdirs(&cfg)? {
        Some(r) => { ack("Recursive", if r { "Yes" } else { "No" }); r }
        None => return Ok(()),
    };

    let delete = match prompt::confirm_delete_originals(&cfg)? {
        Some(d) => { ack("Delete originals", if d { "Yes" } else { "No" }); d }
        None => return Ok(()),
    };

    let subfolder = prompt::select_output_destination()?;
    if let Some(ref s) = subfolder {
        ack("Output sub-folder", s);
    }

    // ── Collect one or more conversion jobs ─────────────────────────
    struct JobSpec {
        media: formats::MediaType,
        input_fmt: &'static str,
        output_fmt: &'static str,
        normalize_audio: bool,
        quality: crate::processor::Quality,
        keep_metadata: bool,
        video_scale: crate::processor::VideoScale,
        image_scale: crate::processor::ImageScale,
    }

    let mut specs: Vec<JobSpec> = Vec::new();

    'outer: loop {
        let media = match prompt::select_media_type()? {
            Some(m) => m,
            None => break,
        };

        // Lazy dep install — only if the tool is actually missing
        if let Err(e) = deps::ensure(media) {
            println!("\n  {} {}\n", style("✗").red(), style(e).red());
            continue;
        }

        let raw_input = match prompt::select_input_format(media)? {
            Some(f) => f,
            None => continue 'outer,
        };

        let mut normalize_audio = false;
        if media == formats::MediaType::Audio {
            normalize_audio = prompt::confirm_audio_normalization()?;
            if normalize_audio {
                ack("Normalize Volume", "Yes (-af loudnorm)");
            }
        }

        // "All Lossless Audio → FLAC" batch shortcut
        if raw_input == formats::LOSSLESS_AUDIO_SENTINEL {
            for &fmt in formats::LOSSLESS_AUDIO_INPUTS {
                specs.push(JobSpec { media: formats::MediaType::Audio, input_fmt: fmt, output_fmt: "FLAC", normalize_audio, quality: prompt::select_quality().unwrap_or(crate::processor::Quality::High), keep_metadata: prompt::confirm_keep_metadata().unwrap_or(true), video_scale: crate::processor::VideoScale::Original, image_scale: crate::processor::ImageScale::Original });
            }
            ack("Added", &format!("{} → FLAC", formats::LOSSLESS_AUDIO_INPUTS.join("/")));
        } else {
            let output_fmt = 'pick_out: loop {
                match prompt::select_output_format(media, raw_input)? {
                    None => continue 'outer,
                    Some(f) => match prompt::lossy_warning(media, raw_input, f)? {
                        Some(true)  => break 'pick_out f,
                        Some(false) => continue,
                        None        => continue 'outer,
                    },
                }
            };
            ack("Output format", output_fmt);
            specs.push(JobSpec { 
                media, 
                input_fmt: raw_input, 
                output_fmt, 
                normalize_audio,
                quality: prompt::select_quality()?,
                keep_metadata: prompt::confirm_keep_metadata()?,
                video_scale: if media == formats::MediaType::Video { prompt::select_video_scale()? } else { crate::processor::VideoScale::Original },
                image_scale: if media == formats::MediaType::Images { prompt::select_image_scale()? } else { crate::processor::ImageScale::Original },
            });
        }

        // Cap at 8 jobs; ask about adding more
        if specs.len() >= 8 || !prompt::confirm_add_another()? {
            break;
        }
    }

    if specs.is_empty() {
        return Ok(());
    }

    // ── Summary ─────────────────────────────────────────────────────
    println!();
    let sep = style("─".repeat(50)).dim();
    println!("  {sep}");
    for s in &specs {
        println!(
            "  {} {} {} → {}",
            style("┃").dim(),
            style(s.media).cyan().bold(),
            style(s.input_fmt).white().bold(),
            style(s.output_fmt).green().bold(),
        );
    }
    println!(
        "  {} {:<18} {}",
        style("┃").dim(),
        style("Directory").dim(),
        style(folder.display()).white(),
    );
    let mut opts = format!("recursive={}  delete_originals={}", recursive, delete);
    if cli.dry_run { opts.push_str("  dry_run=true"); }
    if let Some(ref s) = subfolder { opts.push_str(&format!("  sub-folder={s}")); }
    println!("  {} {:<18} {}", style("┃").dim(), style("Options").dim(), style(&opts).white());
    println!("  {sep}\n");

    match prompt::final_confirmation()? {
        Some(true) => {}
        _ => { println!("  {} Cancelled.", style("✗").red()); return Ok(()); }
    }

    let threads = util::active_threads();

    // ── Run all jobs ─────────────────────────────────────────────────
    let mut summaries = Vec::new();
    for s in &specs {
        if specs.len() > 1 {
            println!(
                "\n  {} {} {} → {}",
                style("→").cyan().bold(),
                s.media.icon(),
                style(s.input_fmt).white().bold(),
                style(s.output_fmt).green().bold(),
            );
        }
        let summary = processor::run(&processor::Job {
            folder: &folder,
            media_type: s.media,
            input_fmt: s.input_fmt,
            output_fmt: s.output_fmt,
            recursive,
            delete_originals: delete,
            dry_run: cli.dry_run,
            threads,
            output_subfolder: subfolder.as_deref(),
            min_file_size: min_size,
            max_file_size: max_size,
            conflict_strategy: cfg.conflict_strategy,
            normalize_audio: s.normalize_audio,
            quality: crate::processor::Quality::High,
            keep_metadata: true,
            video_scale: crate::processor::VideoScale::Original,
            image_scale: crate::processor::ImageScale::Original,
        })?;
        summaries.push(summary);
    }

    if cli.json {
        if let Ok(j) = serde_json::to_string_pretty(&summaries) {
            println!("{}", j);
        }
    }

    println!("\n  {} Done!", style("✔").green().bold());
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn banner() {
    println!();
    println!(
        "  {}  {}",
        style("🔧").cyan(),
        style("rusty-crunch").cyan().bold(),
    );
    println!(
        "     {}",
        style("fast · parallel · media converter").dim(),
    );
    println!();
}

fn ack(label: &str, value: &str) {
    println!(
        "  {} {:<16} {}",
        style("✓").green(),
        style(label).dim(),
        style(value).white().bold(),
    );
}

fn maybe_clear(mode: config::DisplayMode) {
    if mode == config::DisplayMode::Clean {
        // Use the console crate's clear which handles Windows and Unix.
        // On Windows cmd.exe we also try the `cls` fallback.
        let term = console::Term::stdout();
        if term.clear_screen().is_err() {
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "cls"])
                    .status();
            }
        }
    }
}

/// Pause so the user can read results before the screen clears.
fn pause_before_menu() {
    use std::io::{self, Write};
    println!();
    print!("  Press Enter to return to the menu...");
    let _ = io::stdout().flush();
    let _ = io::stdin().read_line(&mut String::new());
}

fn run_recommended_crunch(cli: &Cli) -> Result<()> {
    let cfg = config::load();

    println!(
        "\n  {} {}\n",
        style("🚀").cyan(),
        style("Recommended Crunch").cyan().bold(),
    );
    println!(
        "  {}",
        style("Converts files to the most efficient format per type:").dim(),
    );
    println!(
        "  {} Audio: WAV → FLAC · MP3/OGG/AAC/M4A/WMA → OPUS",
        style("·").dim(),
    );
    println!(
        "  {} Video: AVI/MOV/FLV/WMV/TS → MKV",
        style("·").dim(),
    );
    println!(
        "  {} Images: BMP/TIFF/ICO/GIF → PNG · JPEG → AVIF",
        style("·").dim(),
    );
    println!(
        "  {} Documents: PDF → PDF (Optimized)",
        style("·").dim(),
    );
    println!();

    // ── Folder ──────────────────────────────────────────────────────
    let folder = if let Some(ref f) = cli.folder {
        let f = if f.is_relative() {
            std::env::current_dir()?.join(f)
        } else {
            f.clone()
        };
        if !f.is_dir() {
            anyhow::bail!("Not a directory: {}", f.display());
        }
        ack("Folder", &f.display().to_string());
        f
    } else {
        match prompt::select_folder(&cfg)? {
            Some(f) => {
                ack("Folder", &f.display().to_string());
                f
            }
            None => return Ok(()),
        }
    };

    // ── Recursive ───────────────────────────────────────────────────
    let recursive = match prompt::confirm_scan_subdirs(&cfg)? {
        Some(r) => {
            ack("Recursive", if r { "Yes" } else { "No" });
            r
        }
        None => return Ok(()),
    };

    // ── Delete originals ────────────────────────────────────────────
    let delete = match prompt::confirm_delete_originals(&cfg)? {
        Some(d) => {
            ack("Delete originals", if d { "Yes" } else { "No" });
            d
        }
        None => return Ok(()),
    };

    let subfolder = prompt::select_output_destination()?;
    if let Some(ref s) = subfolder {
        ack("Output sub-folder", s);
    }

    // ── Scan for applicable conversions ─────────────────────────────
    let all_conversions = formats::recommended_conversions();
    let applicable: Vec<(formats::MediaType, &str, &str)> = all_conversions
        .iter()
        .copied()
        .filter(|(_, input_fmt, _)| {
            processor::has_matching_files(&folder, input_fmt, recursive)
        })
        .collect();

    if applicable.is_empty() {
        println!(
            "\n  {} No files found that can be optimized in {}",
            style("⚠").yellow(),
            style(folder.display()).dim(),
        );
        return Ok(());
    }

    // ── Summary ─────────────────────────────────────────────────────
    println!();
    let sep = style("─".repeat(50)).dim();
    println!("  {sep}");
    for &(mt, inf, outf) in &applicable {
        println!(
            "  {} {} {} → {}",
            style("┃").dim(),
            mt.icon(),
            style(inf).white().bold(),
            style(outf).green().bold(),
        );
    }
    if cli.dry_run {
        println!(
            "  {} {}",
            style("┃").dim(),
            style("dry_run=true").white(),
        );
    }
    println!("  {sep}");
    println!();

    // ── Confirm ─────────────────────────────────────────────────────
    match prompt::final_confirmation()? {
        Some(true) => {}
        _ => {
            println!("  {} Cancelled.", style("✗").red());
            return Ok(());
        }
    }

    // ── Ensure dependencies ─────────────────────────────────────────
    let mut ensured: Vec<formats::MediaType> = Vec::new();
    for &(mt, _, _) in &applicable {
        if !ensured.contains(&mt) {
            deps::ensure(mt)?;
            ensured.push(mt);
        }
    }

    // ── Run conversions ─────────────────────────────────────────────
    let threads = util::active_threads();
    let mut summaries = Vec::new();
    for &(media_type, input_fmt, output_fmt) in &applicable {
        println!(
            "\n  {} {} → {}",
            style("→").cyan().bold(),
            style(input_fmt).white().bold(),
            style(output_fmt).green().bold(),
        );
        let summary = processor::run(&processor::Job {
            folder: &folder,
            media_type,
            input_fmt,
            output_fmt,
            recursive,
            delete_originals: delete,
            dry_run: cli.dry_run,
            threads,
            output_subfolder: subfolder.as_deref(),
            min_file_size: None,
            max_file_size: None,
            conflict_strategy: cfg.conflict_strategy,
            normalize_audio: false,
            quality: crate::processor::Quality::High,
            keep_metadata: true,
            video_scale: crate::processor::VideoScale::Original,
            image_scale: crate::processor::ImageScale::Original,
        })?;
        summaries.push(summary);
    }

    if cli.json {
        if let Ok(j) = serde_json::to_string_pretty(&summaries) {
            println!("{}", j);
        }
    }

    println!(
        "\n  {} Recommended Crunch complete!",
        style("✔").green().bold(),
    );
    Ok(())
}

fn check_for_updates() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    println!(
        "\n  {} Checking for updates (current: v{}) \u{2026}",
        style("🔄").cyan(),
        style(current).dim(),
    );

    match util::check_for_update() {
        Err(e) => {
            println!("  {} {}\n", style("⚠").yellow(), style(e).dim());
        }
        Ok(None) => {
            println!(
                "  {} Already up to date (v{})\n",
                style("✓").green(),
                current,
            );
        }
        Ok(Some(latest)) => {
            println!(
                "  {} Update available: v{}\n",
                style("🆕").cyan(),
                style(&latest).cyan().bold(),
            );
            let prompt = format!("Update from v{current} to v{latest}?");
            use dialoguer::Confirm;
            use dialoguer::theme::ColorfulTheme;
            match Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(&prompt)
                .default(true)
                .interact_opt()?
            {
                Some(true) => util::download_and_install_update(&latest)?,
                _ => println!("  {} Skipped\n", style("·").dim()),
            }
        }
    }
    Ok(())
}

fn run_health_check() -> Result<()> {
    println!("\n  {} {}\n", style("🔍").cyan(), style("System Health Check").cyan().bold());

    let mut issues = Vec::new();

    // Check ffmpeg (used by audio/video)
    if !util::has("ffmpeg") {
        issues.push(("ffmpeg", "Audio/Video conversion"));
    }

    // Check ImageMagick (used by images)
    if !util::has_magick() {
        issues.push(("ImageMagick (magick)", "Image conversion"));
    }

    // Check Ghostscript (used by PDF)
    if !util::has_gs() {
        issues.push(("Ghostscript (gs)", "PDF optimization"));
    }

    // Check LibreOffice (used by documents)
    if !util::has_lo() {
        issues.push(("LibreOffice", "Document conversion"));
    }

    if issues.is_empty() {
        println!("  {} All required tools are installed:\n", style("✓").green());
        println!("  {} ffmpeg — audio/video", style("✓").green());
        println!("  {} ImageMagick — images", style("✓").green());
        println!("  {} Ghostscript — PDF", style("✓").green());
        println!("  {} LibreOffice — documents", style("✓").green());
        println!("\n  {} System is ready for conversions\n", style("✓").green().bold());
        return Ok(());
    }

    println!("  {} {} tool{} missing:\n", style("✗").red(), issues.len(), if issues.len() == 1 { "" } else { "s" });
    for (tool, purpose) in &issues {
        println!("  {} {} — {}", style("✗").red(), style(tool).white().bold(), purpose);
    }

    println!("\n  {} Install missing tools:\n", style("ℹ").cyan());
    println!("  {} rusty-crunch can auto-install on supported systems", style("·").dim());
    println!("  {} Or install manually: https://github.com/pablogonz12/rusty-crunch#installation\n", style("·").dim());

    Ok(())
}
