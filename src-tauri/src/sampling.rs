//! Where each sample of the species you're sampling was taken, so the overlay can show how far you are from them.
//!
//! `ScanOrganic` has no coordinates, so each sample is pinned to the position in `Status.json` when the event
//! arrives. Samples taken while the app wasn't running have no position. The trail is saved so a restart mid-species
//! keeps it.

use crate::bio;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "sampling.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Trail {
    pub system_address: u64,
    pub body_id: u32,
    pub species_id: String,
    pub species: String,
    pub colony_distance_m: Option<u32>,
    /// `[latitude, longitude]` of each sample so far, oldest first; `None` where the position isn't known.
    pub samples: Vec<Option<[f64; 2]>>,
    /// Journal timestamp of the latest sample, so replaying the journal at startup doesn't count it twice.
    pub updated: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ScanOrganicEvent {
    #[serde(rename = "timestamp")]
    timestamp: String,
    scan_type: String,
    genus: String,
    species: String,
    #[serde(rename = "Species_Localised")]
    species_localised: Option<String>,
    system_address: u64,
    body: u32,
}

pub struct Sampling {
    trail: Option<Trail>,
    path: Option<PathBuf>,
}

impl Sampling {
    pub fn open(path: Option<PathBuf>) -> Self {
        let trail = path.as_deref().and_then(load);
        Self { trail, path }
    }

    pub fn trail(&self) -> Option<&Trail> {
        self.trail.as_ref()
    }

    /// Applies one journal event; `pos` is where you are now, if known. Returns true if the trail changed.
    pub fn apply(&mut self, ev: &Value, pos: Option<[f64; 2]>) -> bool {
        let changed = match ev.get("event").and_then(Value::as_str) {
            Some("ScanOrganic") => self.apply_scan(ev, pos),
            Some("Died") => self.trail.take().is_some(),
            _ => false,
        };
        if changed {
            if let Some(path) = &self.path {
                save(path, self.trail.as_ref());
            }
        }
        changed
    }

    fn apply_scan(&mut self, ev: &Value, pos: Option<[f64; 2]>) -> bool {
        let Ok(e) = ScanOrganicEvent::deserialize(ev) else { return false };
        if self.trail.as_ref().is_some_and(|t| e.timestamp <= t.updated) {
            return false;
        }
        let same_species = |t: &Trail| t.system_address == e.system_address && t.body_id == e.body && t.species_id == e.species;
        match e.scan_type.as_str() {
            "Log" => {
                self.trail = Some(Trail {
                    system_address: e.system_address,
                    body_id: e.body,
                    colony_distance_m: bio::colony_distance_m(&e.genus),
                    species: e.species_localised.unwrap_or_else(|| e.species.clone()),
                    species_id: e.species,
                    samples: vec![pos],
                    updated: e.timestamp,
                });
            }
            "Sample" => match self.trail.as_mut().filter(|t| same_species(t)) {
                Some(t) => {
                    t.samples.push(pos);
                    t.updated = e.timestamp;
                }
                // The first sample happened before this app saw it.
                None => {
                    self.trail = Some(Trail {
                        system_address: e.system_address,
                        body_id: e.body,
                        colony_distance_m: bio::colony_distance_m(&e.genus),
                        species: e.species_localised.unwrap_or_else(|| e.species.clone()),
                        species_id: e.species,
                        samples: vec![None, pos],
                        updated: e.timestamp,
                    });
                }
            },
            // The third sample completes the species; there's nothing left to keep your distance from.
            "Analyse" => return self.trail.take().is_some(),
            _ => return false,
        }
        true
    }
}

fn load(path: &Path) -> Option<Trail> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn save(path: &Path, trail: Option<&Trail>) {
    let result = match trail {
        Some(t) => {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            serde_json::to_string(t).map_err(std::io::Error::other).and_then(|json| fs::write(path, json))
        }
        None => match fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
    };
    if let Err(e) = result {
        eprintln!("failed to save sampling trail to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(ts: &str, kind: &str, species: &str) -> Value {
        serde_json::from_str(&format!(
            r#"{{ "timestamp":"{ts}", "event":"ScanOrganic", "ScanType":"{kind}", "Genus":"$Codex_Ent_Stratum_Genus_Name;", "Species":"{species}", "Species_Localised":"Stratum Tectonicas", "SystemAddress":564387064497, "Body":10 }}"#
        ))
        .unwrap()
    }

    #[test]
    fn records_each_sample_until_analysed() {
        let mut s = Sampling::open(None);
        assert!(s.apply(&scan("2026-09-21T22:50:00Z", "Log", "a"), Some([1.0, 2.0])));
        assert!(s.apply(&scan("2026-09-21T22:52:00Z", "Sample", "a"), Some([1.1, 2.0])));
        let t = s.trail().unwrap();
        assert_eq!(t.samples, vec![Some([1.0, 2.0]), Some([1.1, 2.0])]);
        assert_eq!(t.colony_distance_m, Some(500));
        assert!(s.apply(&scan("2026-09-21T22:54:00Z", "Analyse", "a"), None));
        assert!(s.trail().is_none());
    }

    #[test]
    fn replayed_events_are_not_counted_twice() {
        let mut s = Sampling::open(None);
        let log = scan("2026-09-21T22:50:00Z", "Log", "a");
        s.apply(&log, Some([1.0, 2.0]));
        assert!(!s.apply(&log, None));
        assert_eq!(s.trail().unwrap().samples.len(), 1);
    }

    #[test]
    fn logging_another_species_starts_over_and_unseen_first_samples_are_unknown() {
        let mut s = Sampling::open(None);
        s.apply(&scan("2026-09-21T22:50:00Z", "Log", "a"), Some([1.0, 2.0]));
        s.apply(&scan("2026-09-21T22:51:00Z", "Log", "b"), Some([3.0, 4.0]));
        assert_eq!(s.trail().unwrap().species_id, "b");
        s.apply(&scan("2026-09-21T22:52:00Z", "Sample", "c"), Some([5.0, 6.0]));
        assert_eq!(s.trail().unwrap().samples, vec![None, Some([5.0, 6.0])]);
    }
}
