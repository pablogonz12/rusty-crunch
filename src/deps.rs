use crate::formats::MediaType;
use crate::util;
use anyhow::{bail, Result};
use console::style;
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
        bail!("Still missing after install: {}", still_missing.join(", "));
    }

    Ok(())
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
