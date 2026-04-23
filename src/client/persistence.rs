use crate::client::app::App;
use crate::error::{MatoError, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedTab {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub preset: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedDesk {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub tabs: Vec<SavedTab>,
    pub active_tab: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SavedOffice {
    pub id: String,
    pub name: String,
    pub desks: Vec<SavedDesk>,
    pub active_desk: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SavedState {
    pub offices: Vec<SavedOffice>,
    #[serde(default)]
    pub current_office: usize,
    #[serde(default)]
    pub current_terminal_preset: Option<String>,
    #[serde(default = "default_alarm_enabled")]
    pub alarm_enabled: bool,
}

fn default_alarm_enabled() -> bool {
    true
}

pub fn save_state(app: &App) -> Result<()> {
    let state = SavedState {
        offices: app
            .offices
            .iter()
            .map(|o| SavedOffice {
                id: o.id.clone(),
                name: o.name.clone(),
                desks: o
                    .desks
                    .iter()
                    .map(|d| SavedDesk {
                        id: d.id.clone(),
                        name: d.name.clone(),
                        cwd: d.cwd.clone(),
                        tabs: d
                            .tabs
                            .iter()
                            .map(|tb| SavedTab {
                                id: tb.id.clone(),
                                name: tb.name.clone(),
                                preset: tb.preset_name.clone(),
                            })
                            .collect(),
                        active_tab: d.active_tab,
                    })
                    .collect(),
                active_desk: o.active_desk,
            })
            .collect(),
        current_office: app.current_office,
        current_terminal_preset: app.current_terminal_preset.clone(),
        alarm_enabled: app.alarm_enabled,
    };

    let json = serde_json::to_string_pretty(&state)?;
    let path = crate::utils::get_state_file_path();

    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MatoError::StateSaveFailed(format!("Cannot create directory: {}", e)))?;
    }

    std::fs::write(&path, json).map_err(|e| {
        MatoError::StateSaveFailed(format!("Cannot write to {}: {}", path.display(), e))
    })?;

    Ok(())
}

pub fn load_state() -> Result<SavedState> {
    let path = crate::utils::get_state_file_path();

    let json = std::fs::read_to_string(&path).map_err(|e| {
        MatoError::StateLoadFailed(format!("Cannot read {}: {}", path.display(), e))
    })?;

    serde_json::from_str(&json).map_err(|e| {
        MatoError::StateParseFailed(format!("Invalid JSON in {}: {}", path.display(), e))
    })
}
