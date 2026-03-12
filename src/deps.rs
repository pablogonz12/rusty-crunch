use crate::formats::MediaType;
use crate::util;
use anyhow::{bail, Result};
use console::style;
use dialoguer::{theme::ColorfulTheme, MultiSelect};
use std::process::{Command, Stdio};

/// Detect the system package manager and return (install-cmd-prefix, package-map).
fn detect_pm() -> Option<(&'static str, fn(&'static str) -> &'static str)> {
    #[cfg(target_os = "windows")]
    {
        if util::has("winget") {
            Some(("winget install --accept-source-agreements --accept-package-agreements", winget_pkg))
        } else if util::has("choco") {
            Some(("choco install -y", choco_pkg))
        } else if util::has("scoop") {
            Some(("scoop install", scoop_pkg))
        } else {
            None
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if util::has("dnf") {
            Some(("sudo dnf install -y", dnf_pkg))
        } else if util::has("apt-get") {
            Some(("sudo apt-get install -y", apt_pkg))
        } else if util::has("pacman") {
            Some(("sudo pacman -S --noconfirm", pacman_pkg))
        } else if util::has("zypper") {
            Some(("sudo zypper install -y", zypper_pkg))
        } else if util::has("brew") {
            Some(("brew install", brew_pkg))
        } else {
            None
        }
    }
}

// ── Package name maps per package manager ──────────────────────────────

fn dnf_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "ImageMagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice",
        _ => tool,
    }
}
fn apt_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "imagemagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice",
        _ => tool,
    }
}
fn pacman_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "imagemagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice-fresh",
        _ => tool,
    }
}
fn zypper_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "ImageMagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice",
        _ => tool,
    }
}
fn brew_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "imagemagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice",
        _ => tool,
    }
}
#[cfg(target_os = "windows")]
fn winget_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "Gyan.FFmpeg",
        "magick" | "convert" => "ImageMagick.ImageMagick",
        "gs" => "ArtifexSoftware.GhostScript",
        "libreoffice" => "TheDocumentFoundation.LibreOffice",
        _ => tool,
    }
}
#[cfg(target_os = "windows")]
fn choco_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "imagemagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice-fresh",
        _ => tool,
    }
}
#[cfg(target_os = "windows")]
fn scoop_pkg(tool: &'static str) -> &'static str {
    match tool {
        "ffmpeg" => "ffmpeg",
        "magick" | "convert" => "imagemagick",
        "gs" => "ghostscript",
        "libreoffice" => "libreoffice",
        _ => tool,
    }
}

/// Run a shell command for installing a package.
fn run_install(full_cmd: &str) -> Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("cmd")
            .args(["/C", full_cmd])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        Ok(status.success())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("sh")
            .args(["-c", full_cmd])
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        Ok(status.success())
    }
}

/// Check that all tools needed for `media` are present.
/// If any are missing, attempt an automatic install via the system package manager.
pub fn ensure(media: MediaType) -> Result<()> {
    // Build the list of missing tools using platform-aware detection.
    // Keys here are logical names for the package-map lookup, not binary names.
    let missing: Vec<&str> = match media {
        MediaType::Audio | MediaType::Video => {
            if util::has("ffmpeg") { vec![] } else { vec!["ffmpeg"] }
        }
        MediaType::Images => {
            // On Windows, `convert` is a built-in disk utility — NEVER use it.
            if util::has_magick() { vec![] } else { vec!["magick"] }
        }
        MediaType::Documents => {
            let mut m = Vec::new();
            if !util::has_gs() { m.push("gs"); }
            if !util::has_lo() { m.push("libreoffice"); }
            m
        }
    };

    if missing.is_empty() {
        return Ok(());
    }

    let (pm_cmd, pkg_fn) = match detect_pm() {
        Some(pm) => pm,
        None => {
            let names: Vec<&str> = missing.iter().copied().collect();
            bail!(
                "Missing tools: {}. No supported package manager found — please install them manually.",
                names.join(", ")
            );
        }
    };

    for tool in &missing {
        let pkg = pkg_fn(tool);
        println!(
            "  {} Installing {} …",
            style("📦").cyan(),
            style(pkg).white().bold(),
        );

        let full = format!("{pm_cmd} {pkg}");
        if !run_install(&full)? {
            bail!(
                "Failed to install `{pkg}`. Try running manually:\n  {full}"
            );
        }

        println!(
            "  {} {} installed",
            style("✓").green(),
            style(pkg).white().bold(),
        );
    }

    // Flush the lookup cache so freshly installed tools are detected
    util::clear_has_cache();

    // Verify after install
    let still_missing: Vec<&str> = match media {
        MediaType::Audio | MediaType::Video => {
            if util::has("ffmpeg") { vec![] } else { vec!["ffmpeg"] }
        }
        MediaType::Images => {
            if util::has_magick() { vec![] } else { vec!["imagemagick"] }
        }
        MediaType::Documents => {
            let mut m = Vec::new();
            if !util::has_gs() { m.push("ghostscript"); }
            if !util::has_lo() { m.push("libreoffice"); }
            m
        }
    };

    if !still_missing.is_empty() {
        #[cfg(target_os = "windows")]
        {
            // On Windows, newly installed tools often aren't visible to the
            // current process because PATH updates require a new shell.
            println!();
            println!(
                "  {} {}",
                style("✓").green().bold(),
                style("Tools were installed successfully!").green(),
            );
            println!(
                "  {} {}",
                style("ℹ").cyan(),
                style("Please restart rusty-crunch so it can detect the new tools.").white().bold(),
            );
            println!();
            wait_for_enter();
            std::process::exit(0);
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Unix-like systems, tools should be detected immediately after install
            let missing_str = still_missing.join(", ");
            bail!(
                "Failed to install required tool(s): {}. Please check your package manager logs.",
                missing_str
            );
        }
    }

    Ok(())
}

/// Pause until the user presses Enter.
fn wait_for_enter() {
    use std::io::{self, Write};
    print!("  Press Enter to continue...");
    let _ = io::stdout().flush();
    let _ = io::stdin().read_line(&mut String::new());
}

/// Verify that tools for `media` are present without attempting auto-install.
/// Used by the headless agent where interactive installation is not possible.
pub fn check(media: MediaType) -> Result<()> {
    let missing: Vec<&str> = match media {
        MediaType::Audio | MediaType::Video => {
            if util::has("ffmpeg") { vec![] } else { vec!["ffmpeg"] }
        }
        MediaType::Images => {
            if util::has_magick() { vec![] } else { vec!["ImageMagick (magick)"] }
        }
        MediaType::Documents => {
            let mut m = Vec::new();
            if !util::has_gs() { m.push("Ghostscript (gs)"); }
            if !util::has_lo() { m.push("LibreOffice"); }
            m
        }
    };
    if missing.is_empty() {
        Ok(())
    } else {
        bail!(
            "Missing required tools: {}. Install them before running the agent.",
            missing.join(", ")
        )
    }
}

// ── Uninstall / clean ───────────────────────────────────────────────────────────────────────

fn uninstall_prefix(install_prefix: &str) -> String {
    if install_prefix.contains("winget") {
        "winget uninstall".into()
    } else if install_prefix.contains("choco") {
        "choco uninstall -y".into()
    } else if install_prefix.contains("scoop") {
        "scoop uninstall".into()
    } else if install_prefix.contains("dnf") {
        "sudo dnf remove -y".into()
    } else if install_prefix.contains("apt-get") {
        "sudo apt-get remove -y".into()
    } else if install_prefix.contains("pacman") {
        "sudo pacman -Rs --noconfirm".into()
    } else if install_prefix.contains("zypper") {
        "sudo zypper remove -y".into()
    } else if install_prefix.contains("brew") {
        "brew uninstall".into()
    } else {
        install_prefix.to_string()
    }
}

/// Get the correct package name for a tool based on the package manager.
/// This maps the tool key (ffmpeg, magick, gs, libreoffice) to the distro-specific package name.
fn get_package_name(pm_prefix: &str, tool_key: &'static str) -> &'static str {
    if pm_prefix.contains("dnf") {
        dnf_pkg(tool_key)
    } else if pm_prefix.contains("apt-get") {
        apt_pkg(tool_key)
    } else if pm_prefix.contains("pacman") {
        pacman_pkg(tool_key)
    } else if pm_prefix.contains("zypper") {
        zypper_pkg(tool_key)
    } else if pm_prefix.contains("brew") {
        brew_pkg(tool_key)
    } else if pm_prefix.contains("winget") {
        #[cfg(target_os = "windows")]
        {
            winget_pkg(tool_key)
        }
        #[cfg(not(target_os = "windows"))]
        {
            tool_key
        }
    } else if pm_prefix.contains("choco") {
        #[cfg(target_os = "windows")]
        {
            choco_pkg(tool_key)
        }
        #[cfg(not(target_os = "windows"))]
        {
            tool_key
        }
    } else if pm_prefix.contains("scoop") {
        #[cfg(target_os = "windows")]
        {
            scoop_pkg(tool_key)
        }
        #[cfg(not(target_os = "windows"))]
        {
            tool_key
        }
    } else {
        tool_key
    }
}

/// Interactively uninstall managed tools (ffmpeg, ImageMagick, Ghostscript, LibreOffice).
pub fn clean_installed() -> Result<()> {
    println!(
        "\n  {} {}\n",
        style("🗑").cyan(),
        style("Clean / Uninstall Tools").cyan().bold(),
    );

    type ToolEntry = (&'static str, bool, &'static str);
    let tools: Vec<ToolEntry> = vec![
        ("ffmpeg",       util::has("ffmpeg"),   "ffmpeg"),
        ("ImageMagick",  util::has_magick(),    "magick"),
        ("Ghostscript",  util::has_gs(),        "gs"),
        ("LibreOffice",  util::has_lo(),        "libreoffice"),
    ];

    let installed: Vec<(&str, &str)> = tools.iter()
        .filter(|(_, present, _)| *present)
        .map(|(name, _, pkg)| (*name, *pkg))
        .collect();

    if installed.is_empty() {
        println!(
            "  {} No managed tools are currently installed.\n",
            style("\u{2713}").green(),
        );
        return Ok(());
    }

    let display: Vec<String> = installed.iter()
        .map(|(name, _)| format!("  {name}"))
        .collect();

    let selections = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select tools to uninstall (Space to toggle, Enter to confirm)")
        .items(&display)
        .interact_opt()?;

    let selections = match selections {
        None => return Ok(()),
        Some(s) if s.is_empty() => {
            println!("  \u{00b7} Nothing selected.\n");
            return Ok(());
        }
        Some(s) => s,
    };

    let (pm_cmd, _) = match detect_pm() {
        Some(pm) => pm,
        None => bail!("No supported package manager found"),
    };
    let uprefix = uninstall_prefix(pm_cmd);

    for idx in selections {
        let (name, tool_key) = installed[idx];
        let pkg_name = get_package_name(pm_cmd, tool_key);
        println!(
            "\n  {} Uninstalling {} \u{2026}",
            style("🗑").cyan(),
            style(name).white().bold(),
        );
        let full = format!("{uprefix} {pkg_name}");
        if run_install(&full)? {
            println!("  {} {} removed", style("\u{2713}").green(), style(name).white());
        } else {
            println!("  {} Failed to remove {}", style("\u{2717}").red(), style(name).red());
        }
    }

    util::clear_has_cache();
    println!();
    Ok(())
}
