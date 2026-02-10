use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, Select, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    Verbose,
    Clean,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Clean
    }
}

impl std::fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Verbose => write!(f, "Verbose"),
            Self::Clean => write!(f, "Clean"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_recursive: bool,
    pub default_delete_originals: bool,
    pub default_folder: Option<String>,
    #[serde(default)]
    pub display_mode: DisplayMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_recursive: true,
            default_delete_originals: false,
            default_folder: None,
            display_mode: DisplayMode::Clean,
        }
    }
}

fn config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rusty-crunch");
    dir.join("config.json")
}

pub fn load() -> Config {
    let path = config_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Config::default()
    }
}

pub fn save(cfg: &Config) -> Result<()> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&path, json)?;
    Ok(())
}

fn theme() -> ColorfulTheme {
    ColorfulTheme::default()
}

pub fn edit_settings() -> Result<()> {
    let mut cfg = load();

    println!(
        "\n  {} {}\n",
        style("⚙").cyan(),
        style("Settings").cyan().bold(),
    );

    let action = Select::with_theme(&theme())
        .with_prompt("What would you like to do?")
        .items(&[
            "✏️  Edit defaults",
            "📤 Export settings",
            "📥 Import settings",
            "🔄 Reset to defaults",
            "↩  Back",
        ])
        .default(0)
        .interact()?;

    match action {
        0 => { /* edit — fall through */ }
        1 => return export_settings(),
        2 => return import_settings(),
        3 => {
            cfg = Config::default();
            save(&cfg)?;
            println!(
                "\n  {} Settings reset to defaults\n",
                style("✓").green(),
            );
            return Ok(());
        }
        _ => return Ok(()),
    }

    cfg.default_recursive = Confirm::with_theme(&theme())
        .with_prompt("Default: scan sub-folders?")
        .default(cfg.default_recursive)
        .interact()?;

    cfg.default_delete_originals = Confirm::with_theme(&theme())
        .with_prompt("Default: delete originals after compression?")
        .default(cfg.default_delete_originals)
        .interact()?;

    let folder_default = cfg.default_folder.clone().unwrap_or_default();
    let folder_input: String = Input::with_theme(&theme())
        .with_prompt("Default folder (leave empty for current directory)")
        .default(folder_default)
        .allow_empty(true)
        .interact_text()?;

    cfg.default_folder = if folder_input.trim().is_empty() {
        None
    } else {
        Some(folder_input.trim().to_string())
    };

    let mode_items = [
        "🔍 Verbose — show everything (best for troubleshooting)",
        "✨ Clean — clear screen between interactions",
    ];
    let mode_default = match cfg.display_mode {
        DisplayMode::Verbose => 0,
        DisplayMode::Clean => 1,
    };
    let mode_idx = Select::with_theme(&theme())
        .with_prompt("Display mode")
        .items(&mode_items)
        .default(mode_default)
        .interact()?;
    cfg.display_mode = match mode_idx {
        1 => DisplayMode::Clean,
        _ => DisplayMode::Verbose,
    };

    save(&cfg)?;

    println!(
        "\n  {} Settings saved to {}\n",
        style("✓").green(),
        style(config_path().display()).dim(),
    );

    Ok(())
}

fn export_settings() -> Result<()> {
    let path = config_path();
    if !path.exists() {
        println!("  {} No settings file found — using defaults", style("⚠").yellow());
        return Ok(());
    }
    let dest: String = Input::with_theme(&theme())
        .with_prompt("Export to (file path)")
        .interact_text()?;
    let dest = dest.trim();
    if dest.is_empty() {
        return Ok(());
    }
    std::fs::copy(&path, dest)?;
    println!(
        "\n  {} Exported to {}\n",
        style("✓").green(),
        style(dest).dim(),
    );
    Ok(())
}

fn import_settings() -> Result<()> {
    let src: String = Input::with_theme(&theme())
        .with_prompt("Import from (file path)")
        .interact_text()?;
    let src = src.trim();
    if src.is_empty() {
        return Ok(());
    }
    let content = std::fs::read_to_string(src)
        .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", src, e))?;
    let _: Config = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Invalid settings file: {}", e))?;
    let dest = config_path();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&dest, &content)?;
    println!(
        "\n  {} Settings imported from {}\n",
        style("✓").green(),
        style(src).dim(),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_display_mode_is_clean() {
        let cfg = Config::default();
        assert_eq!(cfg.display_mode, DisplayMode::Clean);
    }

    #[test]
    fn deserialize_legacy_config_without_display_mode() {
        let json = r#"{
            "default_recursive": true,
            "default_delete_originals": false,
            "default_folder": null
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.display_mode, DisplayMode::Clean);
    }

    #[test]
    fn roundtrip_serialize_config() {
        let cfg = Config {
            default_recursive: false,
            default_delete_originals: true,
            default_folder: Some("/test".into()),
            display_mode: DisplayMode::Verbose,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.display_mode, DisplayMode::Verbose);
        assert_eq!(cfg2.default_folder, Some("/test".into()));
    }
}
