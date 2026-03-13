use crate::config::Config;
use crate::formats::{self, MediaType};
use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
use std::path::PathBuf;

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

/// Strip the `\\?\` extended-length prefix that `canonicalize()` adds on Windows.
/// These paths work for I/O but look confusing to users.
fn clean_path(p: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let s = p.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    p
}

fn ensure_folder_access(path: &std::path::Path) -> Result<()> {
    if !path.is_dir() {
        anyhow::bail!("Not a directory: {}", path.display());
    }
    let _ = std::fs::read_dir(path)
        .map_err(|e| anyhow::anyhow!("Cannot read directory {}: {}", path.display(), e))?;
    Ok(())
}

/// None = user pressed Escape (go back).
pub fn select_media_type() -> Result<Option<MediaType>> {
    let items: Vec<String> = MediaType::ALL.iter().map(|m| m.display_item()).collect();

    let idx = Select::with_theme(&theme())
        .with_prompt("What kind of media are you compressing?  (Esc to go back)")
        .items(&items)
        .default(0)
        .interact_opt()?;

    Ok(idx.map(|i| MediaType::ALL[i]))
}

pub fn select_input_format(media: MediaType) -> Result<Option<&'static str>> {
    let fmts = media.formats();

    // For Audio: prepend a "All Lossless → FLAC" shortcut
    let has_lossless = media == MediaType::Audio;
    let mut items: Vec<&str> = Vec::with_capacity(fmts.len() + 1);
    if has_lossless {
        items.push("★ All Lossless (WAV/AIFF \u{2192} FLAC)");
    }
    items.extend_from_slice(fmts);

    let idx = Select::with_theme(&theme())
        .with_prompt("Select the input format  (Esc to go back)")
        .items(&items)
        .default(if has_lossless { 1 } else { 0 })
        .interact_opt()?;

    match idx {
        None => Ok(None),
        Some(0) if has_lossless => Ok(Some(formats::LOSSLESS_AUDIO_SENTINEL)),
        Some(i) => {
            let offset = if has_lossless { 1 } else { 0 };
            Ok(Some(fmts[i - offset]))
        }
    }
}

pub fn select_output_format(media: MediaType, input_fmt: &str) -> Result<Option<&'static str>> {
    let options = media.compatible_outputs(input_fmt);

    if options.is_empty() {
        anyhow::bail!("No compatible output formats for {} → {}", media, input_fmt);
    }

    let idx = Select::with_theme(&theme())
        .with_prompt("Select the output format  (Esc to go back)")
        .items(&options)
        .default(0)
        .interact_opt()?;

    Ok(idx.map(|i| options[i]))
}

/// Interactive directory browser. Shows `ls`-style listing of sub-dirs,
/// with options to confirm, type/paste a path, or go up.
pub fn select_folder(cfg: &Config) -> Result<Option<PathBuf>> {
    let start = if let Some(ref f) = cfg.default_folder {
        let p = PathBuf::from(f);
        if p.is_dir() {
            match p.canonicalize() {
                Ok(c) => clean_path(c),
                Err(e) => {
                    println!(
                        "  {} Could not canonicalize configured folder ({}): {}",
                        style("⚠").yellow(),
                        p.display(),
                        e
                    );
                    clean_path(p)
                }
            }
        } else {
            std::env::current_dir()?
        }
    } else {
        std::env::current_dir()?
    };

    browse_directory(start)
}

fn browse_directory(start: PathBuf) -> Result<Option<PathBuf>> {
    let mut current = if start.is_dir() {
        clean_path(start.canonicalize()?)
    } else {
        clean_path(std::env::current_dir()?)
    };

    loop {
        println!(
            "\n  {} {}",
            style("📂").cyan(),
            style(current.display()).white().bold(),
        );

        let mut items: Vec<String> = vec![
            "✓ Use this folder".into(),
            "📝 Type / paste a path".into(),
        ];

        // Add parent entry if not at root
        let has_parent = current.parent().is_some();
        if has_parent {
            items.push("⬆  .. (parent directory)".into());
        }

        // List subdirectories sorted
        let mut subdirs: Vec<PathBuf> = std::fs::read_dir(&current)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        subdirs.sort();

        let fixed_count = items.len();
        for d in &subdirs {
            let name = d.file_name().unwrap_or_default().to_string_lossy();
            // Dim hidden dirs
            if name.starts_with('.') {
                items.push(format!("  📁 {}", style(&name).dim()));
            } else {
                items.push(format!("  📁 {name}"));
            }
        }

        let sel = Select::with_theme(&theme())
            .with_prompt("Browse or select folder  (Esc to go back)")
            .items(&items)
            .default(0)
            .interact_opt()?;

        match sel {
            None => return Ok(None), // Escape → back
            Some(0) => return Ok(Some(current)),
            Some(1) => {
                // Type / paste a path
                let input: String = Input::with_theme(&theme())
                    .with_prompt("Path")
                    .default(current.display().to_string())
                    .interact_text()?;
                let p = PathBuf::from(input.trim());
                let p = if p.is_relative() {
                    std::env::current_dir()?.join(p)
                } else {
                    p
                };
                match p.canonicalize() {
                    Ok(p) => {
                        if let Err(e) = ensure_folder_access(&p) {
                            println!("  {} {}", style("✗").red(), e);
                        } else {
                            return Ok(Some(clean_path(p)));
                        }
                    }
                    _ => {
                        println!(
                            "  {} Not a valid directory",
                            style("✗").red()
                        );
                    }
                }
            }
            Some(2) if has_parent => {
                if let Some(parent) = current.parent() {
                    current = parent.to_path_buf();
                }
            }
            Some(i) => {
                let dir_idx = i - fixed_count;
                if dir_idx < subdirs.len() {
                    current = subdirs[dir_idx].clone();
                }
            }
        }
    }
}

pub fn confirm_scan_subdirs(cfg: &Config) -> Result<Option<bool>> {
    Confirm::with_theme(&theme())
        .with_prompt("Scan sub-folders, too?  (Esc to go back)")
        .default(cfg.default_recursive)
        .interact_opt()
        .map_err(Into::into)
}

pub fn confirm_delete_originals(cfg: &Config) -> Result<Option<bool>> {
    Confirm::with_theme(&theme())
        .with_prompt("Delete originals after compression?  (Esc to go back)")
        .default(cfg.default_delete_originals)
        .interact_opt()
        .map_err(Into::into)
}

/// Show a warning when the user is converting between lossy/lossless formats.
/// Returns `Some(true)` if user accepted, `Some(false)` if declined, `None` if Escape.
pub fn lossy_warning(media: MediaType, input: &str, output: &str) -> Result<Option<bool>> {
    if let Some(msg) = media.lossy_warning(input, output) {
        println!(
            "\n  {} {}\n",
            style("⚠").yellow().bold(),
            style(msg).yellow(),
        );
        return Confirm::with_theme(&theme())
            .with_prompt("Continue anyway?  (Esc to go back)")
            .default(false)
            .interact_opt()
            .map_err(Into::into);
    }
    Ok(Some(true))
}

pub fn final_confirmation() -> Result<Option<bool>> {
    Confirm::with_theme(&theme())
        .with_prompt("Press Enter to start crunching  (Esc to cancel)")
        .default(true)
        .interact_opt()
        .map_err(Into::into)
}

/// Ask whether the user wants to queue another file-type conversion in this run.
pub fn confirm_add_another() -> Result<bool> {
    Ok(Confirm::with_theme(&theme())
        .with_prompt("Add another file type to this run?")
        .default(false)
        .interact()
        .unwrap_or(false))
}

/// Validate subfolder names: reject absolute paths, `..` traversal, and path separators.
/// Returns true if safe (relative folder name only).
fn is_safe_subfolder_name(name: &str) -> bool {
    // Reject absolute paths
    if name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    // Reject Windows drive letters (C:, D:, etc.)
    if name.len() >= 2 && name.chars().nth(1) == Some(':') {
        return false;
    }
    // Reject parent directory traversals
    if name.contains("..") {
        return false;
    }
    // Reject path separators
    if name.contains('/') || name.contains('\\') {
        return false;
    }
    // Reject empty or whitespace-only
    !name.trim().is_empty()
}

/// Ask where converted files should be placed.
/// Returns `None` for the same folder (default), or `Some("name")` for a sub-folder.
pub fn select_output_destination() -> Result<Option<String>> {
    let items = [
        "📁 Same folder (alongside originals)",
        "📂 New sub-folder (e.g. \"compressed\")",
    ];
    let sel = Select::with_theme(&theme())
        .with_prompt("Where should converted files be placed?")
        .items(&items)
        .default(0)
        .interact_opt()?;

    match sel {
        None | Some(0) => Ok(None),
        Some(_) => loop {
            let name: String = Input::with_theme(&theme())
                .with_prompt("Sub-folder name")
                .default("compressed".to_string())
                .interact_text()?;
            let trimmed = name.trim().to_string();
            if is_safe_subfolder_name(&trimmed) {
                return Ok(Some(trimmed));
            } else {
                println!(
                    "  {} Invalid folder name. Use only simple folder names (no /\\..)\n",
                    style("⚠").yellow()
                );
            }
        }
    }
}

/// Ask user how to handle existing output files.
#[allow(dead_code)]
pub fn select_conflict_strategy() -> Result<crate::config::ConflictStrategy> {
    use crate::config::ConflictStrategy;

    let items = [
        "⊘ Skip existing files (safest)",
        "⚡ Overwrite existing files",
        "🔄 Rename with suffix (.1, .2, ...)",
    ];

    let sel = Select::with_theme(&theme())
        .with_prompt("What if output file already exists?")
        .items(&items)
        .default(0)
        .interact()?;

    Ok(match sel {
        0 => ConflictStrategy::Skip,
        1 => ConflictStrategy::Overwrite,
        _ => ConflictStrategy::Rename,
    })
}

pub fn confirm_audio_normalization() -> Result<bool> {
    let items = [
        "No, keep original volume",
        "🔊 Yes, normalize audio tracks (Loudness + Peak)",
    ];
    let sel = Select::with_theme(&theme())
        .with_prompt("Do you want to normalize audio volume?")
        .items(&items)
        .default(0)
        .interact_opt()?;
    Ok(sel == Some(1))
}
