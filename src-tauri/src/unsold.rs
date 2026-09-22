//! What you're carrying and haven't sold yet: cartographic data (per system, since Universal Cartographics buys it a
//! system at a time) and analysed exobiology samples. Both are lost if you die.
//!
//! Values are estimates from the same formulas as the rest of the app, not what the game will pay.

use crate::bio;
use crate::values::{self, PlanetValueInput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnsoldView {
    pub carto_value: u64,
    pub carto_systems: usize,
    pub carto_bodies: usize,
    pub bio_value: u64,
    pub bio_species: usize,
}

/// Cheap pre-filter for journal lines this cares about, so old journals don't all need parsing.
pub fn wants(line: &str) -> bool {
    const EVENTS: [&str; 9] = [
        r#""event":"Scan""#, // also matches ScanOrganic
        r#""event":"SAAScanComplete""#,
        r#""event":"SellOrganicData""#,
        r#""event":"SellExplorationData""#,
        r#""event":"MultiSellExplorationData""#,
        r#""event":"Died""#,
        r#""event":"Location""#,
        r#""event":"FSDJump""#,
        r#""event":"CarrierJump""#,
    ];
    EVENTS.iter().any(|e| line.contains(e))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Arrival {
    star_system: String,
    system_address: u64,
    #[serde(default)]
    population: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ScanEvent {
    #[serde(rename = "BodyID")]
    body_id: u32,
    star_system: Option<String>,
    system_address: Option<u64>,
    #[serde(default)]
    scan_type: String,
    star_type: Option<String>,
    stellar_mass: Option<f64>,
    planet_class: Option<String>,
    #[serde(rename = "MassEM")]
    mass_em: Option<f64>,
    terraform_state: Option<String>,
    #[serde(default)]
    was_discovered: bool,
    #[serde(default)]
    was_mapped: bool,
    #[serde(default)]
    was_footfalled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MapCompleteEvent {
    #[serde(rename = "BodyID")]
    body_id: u32,
    system_address: u64,
    probes_used: u32,
    efficiency_target: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ScanOrganicEvent {
    scan_type: String,
    species: String,
    system_address: u64,
    body: u32,
}

enum Valued {
    Star(u64),
    Planet { class: String, terraformable: bool, earth_masses: f64, first_discovery: bool, first_mapped: bool },
}

struct BodyData {
    valued: Valued,
    was_footfalled: bool,
    /// `Some(efficient)` once mapped with the DSS.
    mapped: Option<bool>,
}

impl BodyData {
    fn value(&self) -> u64 {
        match &self.valued {
            Valued::Star(v) => *v,
            Valued::Planet { class, terraformable, earth_masses, first_discovery, first_mapped } => {
                values::planet_value(&PlanetValueInput {
                    planet_class: class,
                    terraformable: *terraformable,
                    earth_masses: *earth_masses,
                    first_discovery: *first_discovery,
                    mapped: self.mapped.is_some(),
                    first_mapped: *first_mapped,
                    efficient: self.mapped == Some(true),
                })
            }
        }
    }
}

#[derive(Default)]
struct SystemData {
    name: String,
    population: u64,
    /// Scanned and not yet sold.
    bodies: HashMap<u32, BodyData>,
    /// Already sold; scanning them again earns nothing more.
    sold: HashSet<u32>,
}

struct Sample {
    species_id: String,
    value: u64,
}

#[derive(Default)]
pub struct Unsold {
    current: Option<u64>,
    systems: HashMap<u64, SystemData>,
    samples: Vec<Sample>,
}

fn parse<T: for<'de> Deserialize<'de>>(ev: &Value) -> Option<T> {
    T::deserialize(ev).ok()
}

impl Unsold {
    /// Applies one journal event. Returns true if the totals may have changed.
    pub fn apply(&mut self, ev: &Value) -> bool {
        match ev.get("event").and_then(Value::as_str) {
            Some("Location" | "FSDJump" | "CarrierJump") => {
                let Some(a) = parse::<Arrival>(ev) else { return false };
                let sys = self.systems.entry(a.system_address).or_default();
                sys.name = a.star_system;
                sys.population = a.population;
                self.current = Some(a.system_address);
                false
            }
            Some("Scan") => {
                let Some(s) = parse::<ScanEvent>(ev) else { return false };
                // Nav beacon data is free and isn't sold to Universal Cartographics.
                if s.scan_type == "NavBeaconDetail" {
                    return false;
                }
                let Some(address) = s.system_address.or(self.current) else { return false };
                let valued = match (&s.star_type, &s.planet_class) {
                    (Some(star), _) => Valued::Star(values::star_value(star, s.stellar_mass.unwrap_or(0.0), !s.was_discovered)),
                    (None, Some(class)) => Valued::Planet {
                        class: class.clone(),
                        terraformable: s.terraform_state.as_deref().is_some_and(|t| !t.is_empty()),
                        earth_masses: s.mass_em.unwrap_or(0.0),
                        first_discovery: !s.was_discovered,
                        first_mapped: !s.was_mapped,
                    },
                    _ => return false,
                };
                let sys = self.systems.entry(address).or_default();
                if let Some(name) = s.star_system {
                    sys.name = name;
                }
                if sys.sold.contains(&s.body_id) {
                    return false;
                }
                let mapped = sys.bodies.get(&s.body_id).and_then(|b| b.mapped);
                sys.bodies.insert(s.body_id, BodyData { valued, was_footfalled: s.was_footfalled, mapped });
                true
            }
            Some("SAAScanComplete") => {
                let Some(e) = parse::<MapCompleteEvent>(ev) else { return false };
                let body = self.systems.get_mut(&e.system_address).and_then(|s| s.bodies.get_mut(&e.body_id));
                match body {
                    Some(b) if b.mapped.is_none() => {
                        b.mapped = Some(e.probes_used <= e.efficiency_target);
                        true
                    }
                    _ => false,
                }
            }
            Some("ScanOrganic") => {
                let Some(e) = parse::<ScanOrganicEvent>(ev) else { return false };
                if e.scan_type != "Analyse" {
                    return false;
                }
                let Some(base) = bio::species_value(&e.species) else { return false };
                // Same rule as the predictions: first-logged bonus on an unwalked body in an unpopulated system.
                let sys = self.systems.get(&e.system_address);
                let first_logged = sys.is_some_and(|s| {
                    s.population == 0 && s.bodies.get(&e.body).is_some_and(|b| !b.was_footfalled)
                });
                let value = if first_logged { base * bio::FIRST_LOGGED_MULTIPLIER } else { base };
                self.samples.push(Sample { species_id: e.species, value });
                true
            }
            Some("SellOrganicData") => {
                let sold = ev.get("BioData").and_then(Value::as_array).into_iter().flatten();
                for item in sold {
                    let species = item.get("Species").and_then(Value::as_str);
                    if let Some(i) = self.samples.iter().position(|s| Some(s.species_id.as_str()) == species) {
                        self.samples.remove(i);
                    }
                }
                true
            }
            Some(kind @ ("SellExplorationData" | "MultiSellExplorationData")) => {
                let names: Vec<&str> = if kind == "SellExplorationData" {
                    ev.get("Systems").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect()
                } else {
                    ev.get("Discovered")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|d| d.get("SystemName").and_then(Value::as_str))
                        .collect()
                };
                for name in names {
                    for sys in self.systems.values_mut().filter(|s| s.name.eq_ignore_ascii_case(name)) {
                        let ids: Vec<u32> = sys.bodies.drain().map(|(id, _)| id).collect();
                        sys.sold.extend(ids);
                    }
                }
                true
            }
            Some("Died") => {
                self.samples.clear();
                for sys in self.systems.values_mut() {
                    sys.bodies.clear();
                }
                true
            }
            _ => false,
        }
    }

    pub fn view(&self) -> UnsoldView {
        let carrying = || self.systems.values().filter(|s| !s.bodies.is_empty());
        UnsoldView {
            carto_value: carrying().flat_map(|s| s.bodies.values()).map(BodyData::value).sum(),
            carto_systems: carrying().count(),
            carto_bodies: carrying().map(|s| s.bodies.len()).sum(),
            bio_value: self.samples.iter().map(|s| s.value).sum(),
            bio_species: self.samples.len(),
        }
    }

    /// Estimated value of the systems in a sale event, for checking the estimate against what the game paid.
    #[cfg(test)]
    pub fn estimate_sale(&self, names: &[&str]) -> u64 {
        self.systems
            .values()
            .filter(|s| names.iter().any(|n| s.name.eq_ignore_ascii_case(n)))
            .flat_map(|s| s.bodies.values())
            .map(BodyData::value)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Unsold {
        let mut u = Unsold::default();
        for l in lines {
            u.apply(&serde_json::from_str(l).unwrap());
        }
        u
    }

    const JUMP: &str = r#"{ "event":"FSDJump", "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "Population":0 }"#;
    const STAR: &str = r#"{ "event":"Scan", "ScanType":"AutoScan", "BodyName":"Smojeia SE-Q d5-3", "BodyID":0, "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "StarType":"F", "StellarMass":1.2, "WasDiscovered":false }"#;
    const PLANET: &str = r#"{ "event":"Scan", "ScanType":"Detailed", "BodyName":"Smojeia SE-Q d5-3 1", "BodyID":12, "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "PlanetClass":"High metal content body", "TerraformState":"Terraformable", "MassEM":0.5, "WasDiscovered":false, "WasMapped":false, "WasFootfalled":false }"#;
    const MAP: &str = r#"{ "event":"SAAScanComplete", "BodyName":"Smojeia SE-Q d5-3 1", "SystemAddress":111979596467, "BodyID":12, "ProbesUsed":5, "EfficiencyTarget":6 }"#;
    const ANALYSE: &str = r#"{ "event":"ScanOrganic", "ScanType":"Analyse", "Genus":"$Codex_Ent_Stratum_Genus_Name;", "Species":"$Codex_Ent_Stratum_07_Name;", "SystemAddress":111979596467, "Body":12 }"#;

    #[test]
    fn counts_scans_mapping_and_samples() {
        let scanned = feed(&[JUMP, STAR, PLANET]).view();
        assert_eq!((scanned.carto_systems, scanned.carto_bodies), (1, 2));
        let mapped = feed(&[JUMP, STAR, PLANET, MAP]).view();
        assert!(mapped.carto_value > scanned.carto_value);

        let bio = feed(&[JUMP, PLANET, ANALYSE]).view();
        assert_eq!(bio.bio_species, 1);
        assert_eq!(bio.bio_value, bio::species_value("$Codex_Ent_Stratum_07_Name;").unwrap() * 5);
    }

    #[test]
    fn selling_clears_only_what_was_sold_and_rescans_earn_nothing() {
        let sell = r#"{ "event":"MultiSellExplorationData", "Discovered":[ { "SystemName":"smojeia se-q d5-3", "NumBodies":2 } ] }"#;
        let u = feed(&[JUMP, STAR, PLANET, ANALYSE, sell]);
        assert_eq!(u.view().carto_value, 0);
        assert_eq!(u.view().bio_species, 1);
        let u = feed(&[JUMP, STAR, PLANET, sell, STAR]);
        assert_eq!(u.view().carto_bodies, 0);

        let sell_bio = r#"{ "event":"SellOrganicData", "BioData":[ { "Species":"$Codex_Ent_Stratum_07_Name;", "Value":1, "Bonus":4 } ] }"#;
        assert_eq!(feed(&[JUMP, PLANET, ANALYSE, ANALYSE, sell_bio]).view().bio_species, 1);
    }

    #[test]
    fn dying_loses_everything() {
        let u = feed(&[JUMP, STAR, PLANET, ANALYSE, r#"{ "event":"Died" }"#]);
        assert_eq!(u.view(), UnsoldView::default());
    }
}
