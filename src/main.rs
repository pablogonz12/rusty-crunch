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

    /// Folder to process (skips the directory browser)
    #[arg(value_name = "FOLDER")]
    folder: Option<PathBuf>,
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

    // ── Step state ──────────────────────────────────────────────────
    let mut media = None;
    let mut input_fmt: Option<&str> = None;
    let mut output_fmt: Option<&str> = None;
    let mut folder: Option<PathBuf> = None;
    let mut recursive = None;
    let mut delete = None;
    let mut step: u8 = 0;

    loop {
        match step {
            // ── Media type ──────────────────────────────────────────
            0 => match prompt::select_media_type()? {
                Some(m) => {
                    ack("Media type", &m.display_item());
                    media = Some(m);
                    step = 1;
                }
                None => return Ok(()), // Esc at first step → back to main menu
            },

            // ── Deps + Input format ─────────────────────────────────
            1 => {
                let m = media.unwrap();
                deps::ensure(m)?;
                match prompt::select_input_format(m)? {
                    Some(f) => {
                        ack("Input format", f);
                        input_fmt = Some(f);
                        step = 2;
                    }
                    None => step = 0,
                }
            }

            // ── Output format + lossy warning ───────────────────────
            2 => {
                let m = media.unwrap();
                let inf = input_fmt.unwrap();
                match prompt::select_output_format(m, inf)? {
                    Some(f) => {
                        // Show lossy warning if applicable
                        match prompt::lossy_warning(m, inf, f)? {
                            Some(true) => {
                                ack("Output format", f);
                                output_fmt = Some(f);
                                step = 3;
                            }
                            Some(false) => {
                                // User declined — re-pick output format
                            }
                            None => step = 1, // Escape → back to input format
                        }
                    }
                    None => step = 1,
                }
            }

            // ── Folder browser ──────────────────────────────────────
            3 => {
                if let Some(ref f) = cli.folder {
                    let f = if f.is_relative() {
                        std::env::current_dir()?.join(f)
                    } else {
                        f.clone()
                    };
                    if f.is_dir() {
                        ack("Folder", &f.display().to_string());
                        folder = Some(f);
                        step = 4;
                    } else {
                        anyhow::bail!("Not a directory: {}", f.display());
                    }
                } else {
                    match prompt::select_folder(&cfg)? {
                        Some(f) => {
                            ack("Folder", &f.display().to_string());
                            folder = Some(f);
                            step = 4;
                        }
                        None => step = 2,
                    }
                }
            }

            // ── Recursive ───────────────────────────────────────────
            4 => match prompt::confirm_scan_subdirs(&cfg)? {
                Some(r) => {
                    ack("Recursive", if r { "Yes" } else { "No" });
                    recursive = Some(r);
                    step = 5;
                }
                None => step = 3,
            },

            // ── Delete originals ────────────────────────────────────
            5 => match prompt::confirm_delete_originals(&cfg)? {
                Some(d) => {
                    ack("Delete originals", if d { "Yes" } else { "No" });
                    delete = Some(d);
                    step = 6;
                }
                None => step = 4,
            },

            // ── Summary + Confirm ───────────────────────────────────
            _ => break,
        }
    }

    // Unwrap all steps (guaranteed by the state machine)
    let media = media.unwrap();
    let input_fmt = input_fmt.unwrap();
    let output_fmt = output_fmt.unwrap();
    let folder = folder.unwrap();
    let recursive = recursive.unwrap();
    let delete = delete.unwrap();

    // ── Summary ─────────────────────────────────────────────────────
    println!();
    let sep = style("─".repeat(50)).dim();
    println!("  {sep}");
    println!(
        "  {} {:<18} {} → {}",
        style("┃").dim(),
        style(media).cyan().bold(),
        style(input_fmt).white().bold(),
        style(output_fmt).green().bold(),
    );
    println!(
        "  {} {:<18} {}",
        style("┃").dim(),
        style("Directory").dim(),
        style(folder.display()).white(),
    );
    let mut opts = format!("recursive={}  delete_originals={}", recursive, delete);
    if cli.dry_run {
        opts.push_str("  dry_run=true");
    }
    println!(
        "  {} {:<18} {}",
        style("┃").dim(),
        style("Options").dim(),
        style(&opts).white(),
    );
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

    // ── Execute ─────────────────────────────────────────────────────
    processor::run(&processor::Job {
        folder: &folder,
        media_type: media,
        input_fmt,
        output_fmt,
        recursive,
        delete_originals: delete,
        dry_run: cli.dry_run,
    })?;

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
    for &(media_type, input_fmt, output_fmt) in &applicable {
        println!(
            "\n  {} {} → {}",
            style("→").cyan().bold(),
            style(input_fmt).white().bold(),
            style(output_fmt).green().bold(),
        );
        processor::run(&processor::Job {
            folder: &folder,
            media_type,
            input_fmt,
            output_fmt,
            recursive,
            delete_originals: delete,
            dry_run: cli.dry_run,
        })?;
    }

    println!(
        "\n  {} Recommended Crunch complete!",
        style("✔").green().bold(),
    );
    Ok(())
}
