use crate::formats::MediaType;
use crate::deps;
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

// ── Thread mode ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThreadMode {
    /// 100% of available cores.
    #[default]
    Full,
    /// 50% of available cores.
    Balanced,
    /// 25% of available cores.
    Saver,
}

impl std::fmt::Display for ThreadMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full     => write!(f, "Power (100% — all cores)"),
            Self::Balanced => write!(f, "Balanced (50% of cores)"),
            Self::Saver    => write!(f, "Power saver (25% of cores)"),
        }
    }
}

impl ThreadMode {
    pub fn to_threads(self, total: usize) -> usize {
        match self {
            Self::Full     => total,
            Self::Balanced => (total / 2).max(1),
            Self::Saver    => ((total + 3) / 4).max(1),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRule {
    pub folder: String,
    pub media_type: MediaType,
    pub input_fmt: String,
    pub output_fmt: String,
    pub recursive: bool,
    pub delete_originals: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentTrigger {
    Watch,
    Periodic(u64),
}

impl Default for AgentTrigger {
    fn default() -> Self {
        Self::Watch
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub default_recursive: bool,
    pub default_delete_originals: bool,
    pub default_folder: Option<String>,
    #[serde(default)]
    pub display_mode: DisplayMode,
    #[serde(default)]
    pub thread_mode: ThreadMode,
    #[serde(default)]
    pub agent_rules: Vec<AgentRule>,
    #[serde(default)]
    pub agent_trigger: AgentTrigger,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_recursive: true,
            default_delete_originals: false,
            default_folder: None,
            display_mode: DisplayMode::Clean,
            thread_mode: ThreadMode::Full,
            agent_rules: Vec::new(),
            agent_trigger: AgentTrigger::default(),
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
            "🗑  Clean / Uninstall tools",
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
        }        4 => return deps::clean_installed(),        _ => return Ok(()),
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

    let thread_items = [
        format!("\u{26a1} {}", ThreadMode::Full),
        format!("\u{2696}  {}", ThreadMode::Balanced),
        format!("🌿 {}", ThreadMode::Saver),
    ];
    let thread_default = match cfg.thread_mode {
        ThreadMode::Full     => 0,
        ThreadMode::Balanced => 1,
        ThreadMode::Saver    => 2,
    };
    let thread_idx = Select::with_theme(&theme())
        .with_prompt("Thread usage")
        .items(&thread_items)
        .default(thread_default)
        .interact()?;
    cfg.thread_mode = match thread_idx {
        1 => ThreadMode::Balanced,
        2 => ThreadMode::Saver,
        _ => ThreadMode::Full,
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
    // Validate that it parses as a valid Config
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
    fn deserialize_config_with_agent_rules() {
        let json = r#"{
            "default_recursive": true,
            "default_delete_originals": false,
            "default_folder": null,
            "display_mode": "Clean",
            "agent_rules": [
                {
                    "folder": "/tmp/test",
                    "media_type": "Audio",
                    "input_fmt": "WAV",
                    "output_fmt": "FLAC",
                    "recursive": true,
                    "delete_originals": false
                }
            ],
            "agent_trigger": "Watch"
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.agent_rules.len(), 1);
        assert_eq!(cfg.agent_rules[0].input_fmt, "WAV");
        assert_eq!(cfg.agent_rules[0].output_fmt, "FLAC");
        assert_eq!(cfg.agent_trigger, AgentTrigger::Watch);
    }

    #[test]
    fn deserialize_periodic_trigger() {
        let json = r#"{
            "default_recursive": true,
            "default_delete_originals": false,
            "default_folder": null,
            "agent_trigger": {"Periodic": 300}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.agent_trigger, AgentTrigger::Periodic(300));
    }

    #[test]
    fn deserialize_legacy_config_without_agent_fields() {
        let json = r#"{
            "default_recursive": true,
            "default_delete_originals": false,
            "default_folder": null,
            "display_mode": "Verbose"
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert!(cfg.agent_rules.is_empty());
        assert_eq!(cfg.agent_trigger, AgentTrigger::Watch);
    }

    #[test]
    fn roundtrip_serialize_config() {
        let cfg = Config {
            default_recursive: false,
            default_delete_originals: true,
            default_folder: Some("/test".into()),
            display_mode: DisplayMode::Clean,
            agent_rules: vec![AgentRule {
                folder: "/tmp".into(),
                media_type: MediaType::Images,
                input_fmt: "BMP".into(),
                output_fmt: "PNG".into(),
                recursive: true,
                delete_originals: false,
            }],
            agent_trigger: AgentTrigger::Periodic(600),
            thread_mode: ThreadMode::Balanced,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.agent_rules.len(), 1);
        assert_eq!(cfg2.agent_trigger, AgentTrigger::Periodic(600));
    }
}
