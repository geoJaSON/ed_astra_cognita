//! Where you've been, where you're going, and rough coordinates for systems you haven't visited.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stop {
    #[serde(rename(deserialize = "StarSystem"))]
    pub name: String,
    #[serde(rename(deserialize = "SystemAddress"))]
    pub address: u64,
    #[serde(rename(deserialize = "StarPos"))]
    pub pos: [f64; 3],
}

/// Every system you've jumped into, oldest first, from all journals.
pub fn history(journal_dir: &Path) -> Vec<Stop> {
    let mut files: Vec<PathBuf> = fs::read_dir(journal_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("Journal.") && n.ends_with(".log")))
        .collect();
    files.sort();
    let mut stops: Vec<Stop> = Vec::new();
    for file in files {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        for line in text.lines() {
            // Cheap filter before parsing; only arrivals carry StarPos for a new system.
            if !(line.contains(r#""event":"FSDJump""#) || line.contains(r#""event":"CarrierJump""#)) {
                continue;
            }
            if let Ok(stop) = serde_json::from_str::<Stop>(line) {
                if stops.last().is_none_or(|last| last.address != stop.address) {
                    stops.push(stop);
                }
            }
        }
    }
    stops
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NavRouteFile {
    #[serde(default)]
    route: Vec<Stop>,
}

/// Re-reads `NavRoute.json`, the route plotted in the galaxy map, whenever the game rewrites it.
pub struct RouteReader {
    path: PathBuf,
    last_modified: Option<SystemTime>,
}

impl RouteReader {
    pub fn new(journal_dir: &Path) -> Self {
        Self { path: journal_dir.join("NavRoute.json"), last_modified: None }
    }

    /// `Some(route)` when the file changed; an empty route means it was cleared.
    pub fn poll(&mut self) -> Option<Vec<Stop>> {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok()?;
        if self.last_modified == Some(modified) {
            return None;
        }
        // A half-written file fails to parse and is retried next tick.
        let file: NavRouteFile = serde_json::from_str(&fs::read_to_string(&self.path).ok()?).ok()?;
        self.last_modified = Some(modified);
        Some(file.route)
    }
}

const BOXEL_X0: f64 = -49985.0;
const BOXEL_Y0: f64 = -40985.0;
const BOXEL_Z0: f64 = -24105.0;

/// Approximate coordinates from a system address (id64), which encodes the sector cube ("boxel") the system is in.
/// Returns the boxel's centre and its edge length in ly (10 for mass code a, up to 1280 for h).
pub fn estimate_position(address: u64) -> ([f64; 3], f64) {
    let mass_code = (address & 7) as u32;
    let size = (10u64 << mass_code) as f64;
    let field = |shift: u32, mask: u64| (((address >> shift) & (mask >> mass_code)) << mass_code) as f64 * 10.0;
    let z = field(3, 0x3FFF) + BOXEL_Z0;
    let y = field(17 - mass_code, 0x1FFF) + BOXEL_Y0;
    let x = field(30 - mass_code * 2, 0x3FFF) + BOXEL_X0;
    ([x + size / 2.0, y + size / 2.0, z + size / 2.0], size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_position_is_within_the_boxel() {
        // From the journal: Smojeia SE-Q d5-3 (mass code d, 80 ly) and Byoi Thaa XW-L c8-3 (mass code c, 40 ly).
        for (address, actual) in [
            (111979596467u64, [-7583.15625, 337.875, 3297.78125]),
            (895467427146, [-7779.46875, 366.3125, 3168.21875]),
        ] {
            let (centre, size) = estimate_position(address);
            for axis in 0..3 {
                assert!((centre[axis] - actual[axis]).abs() <= size / 2.0, "{address}: {centre:?} vs {actual:?}");
            }
        }
    }

    #[test]
    fn parses_nav_route() {
        let json = r#"{ "timestamp":"2026-09-21T15:54:33Z", "event":"NavRoute", "Route":[
            { "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "StarPos":[-7583.15625,337.87500,3297.78125], "StarClass":"F" } ] }"#;
        let file: NavRouteFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.route[0].name, "Smojeia SE-Q d5-3");
        let cleared: NavRouteFile = serde_json::from_str(r#"{ "event":"NavRouteClear", "Route":[] }"#).unwrap();
        assert!(cleared.route.is_empty());
    }
}
