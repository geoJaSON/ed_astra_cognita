use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub overlay_enabled: bool,
    /// Physical pixels, saved when the overlay is re-locked after moving it.
    pub overlay_position: Option<(i32, i32)>,
    /// Planets whose mapped value (with your first bonuses) is below this aren't highlighted.
    pub worth_mapping_min: u64,
    /// Bodies whose best-case exobiology payout is below this aren't highlighted.
    pub worth_bio_min: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self { overlay_enabled: true, overlay_position: None, worth_mapping_min: 300_000, worth_bio_min: 5_000_000 }
    }
}

pub fn load(path: &Path) -> Settings {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        if let Err(e) = fs::write(path, json) {
            eprintln!("failed to save settings to {}: {e}", path.display());
        }
    }
}
