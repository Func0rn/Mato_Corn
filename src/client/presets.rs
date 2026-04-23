use crate::error::{MatoError, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalPreset {
    pub name: String,
    #[serde(default)]
    pub command: String,
}

impl TerminalPreset {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
        }
    }
}

pub fn default_presets() -> Vec<TerminalPreset> {
    vec![
        TerminalPreset::new("zsh", ""),
        TerminalPreset::new("codex-danger", "codex --dangerously-bypass-approvals-and-sandbox"),
    ]
}

pub fn load_presets() -> Vec<TerminalPreset> {
    let path = crate::utils::get_terminal_presets_file_path();
    let Ok(json) = std::fs::read_to_string(&path) else {
        let presets = default_presets();
        let _ = save_presets(&presets);
        return presets;
    };
    match serde_json::from_str::<Vec<TerminalPreset>>(&json) {
        Ok(presets) if !presets.is_empty() => presets,
        _ => default_presets(),
    }
}

pub fn save_presets(presets: &[TerminalPreset]) -> Result<()> {
    let path = crate::utils::get_terminal_presets_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MatoError::StateSaveFailed(format!("Cannot create directory: {}", e)))?;
    }
    let json = serde_json::to_string_pretty(presets)?;
    std::fs::write(&path, json).map_err(|e| {
        MatoError::StateSaveFailed(format!("Cannot write to {}: {}", path.display(), e))
    })
}

pub fn desk_root() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("mato_corn")
}

pub fn existing_desk_names() -> Vec<String> {
    let root = desk_root();
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.file_type().ok()?.is_dir() {
                entry.file_name().to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    names
}

pub fn desk_path_for_name(name: &str) -> std::path::PathBuf {
    desk_root().join(name.trim())
}

