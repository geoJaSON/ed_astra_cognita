//! Estimated Universal Cartographics payouts, driven by `data/body_values.json`.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tables {
    planet: PlanetTable,
    star: StarTable,
    multipliers: Multipliers,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanetTable {
    q: f64,
    default_k: f64,
    default_terraform_bonus: f64,
    classes: HashMap<String, PlanetClass>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanetClass {
    k: f64,
    terraform_bonus: f64,
    #[serde(default)]
    always_terraformable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StarTable {
    default_k: f64,
    mass_divisor: f64,
    classes: HashMap<String, f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Multipliers {
    first_discovery: f64,
    mapped: f64,
    mapped_first_mapped: f64,
    mapped_first_discovered_and_mapped: f64,
    efficiency: f64,
    odyssey_mapped_bonus_fraction: f64,
    odyssey_mapped_bonus_min: f64,
    minimum_value: f64,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../data/body_values.json"))
            .expect("data/body_values.json is malformed")
    })
}

pub fn star_value(star_type: &str, solar_masses: f64, first_discovery: bool) -> u64 {
    let t = tables();
    let k = t.star.classes.get(star_type).copied().unwrap_or(t.star.default_k);
    let mut value = k + solar_masses * k / t.star.mass_divisor;
    if first_discovery {
        value *= t.multipliers.first_discovery;
    }
    value.round() as u64
}

pub struct PlanetValueInput<'a> {
    pub planet_class: &'a str,
    pub terraformable: bool,
    pub earth_masses: f64,
    pub first_discovery: bool,
    pub mapped: bool,
    pub first_mapped: bool,
    pub efficient: bool,
}

pub fn planet_value(p: &PlanetValueInput) -> u64 {
    let t = tables();
    let m = &t.multipliers;
    let (base_k, tf_bonus, always_tf) = match t.planet.classes.get(p.planet_class) {
        Some(c) => (c.k, c.terraform_bonus, c.always_terraformable),
        None => (t.planet.default_k, t.planet.default_terraform_bonus, false),
    };
    let k = if p.terraformable || always_tf { base_k + tf_bonus } else { base_k };

    let mapping_multiplier = match (p.mapped, p.first_discovery, p.first_mapped) {
        (false, _, _) => 1.0,
        (true, true, true) => m.mapped_first_discovered_and_mapped,
        (true, false, true) => m.mapped_first_mapped,
        (true, _, false) => m.mapped,
    };
    let mut value = (k + k * t.planet.q * p.earth_masses.powf(0.2)) * mapping_multiplier;
    if p.mapped {
        value += (value * m.odyssey_mapped_bonus_fraction).max(m.odyssey_mapped_bonus_min);
        if p.efficient {
            value *= m.efficiency;
        }
    }
    value = value.max(m.minimum_value);
    if p.first_discovery {
        value *= m.first_discovery;
    }
    value.round() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planet(class: &str, tf: bool, mass: f64, fd: bool, mapped: bool, fm: bool) -> u64 {
        planet_value(&PlanetValueInput {
            planet_class: class,
            terraformable: tf,
            earth_masses: mass,
            first_discovery: fd,
            mapped,
            first_mapped: fm,
            efficient: true,
        })
    }

    #[test]
    fn earthlike_first_discovered_and_mapped_is_millions() {
        let v = planet("Earthlike body", false, 1.0, true, true, true);
        assert!((3_000_000..6_000_000).contains(&v), "got {v}");
    }

    #[test]
    fn mapping_and_first_discovery_increase_value() {
        let scan_only = planet("High metal content body", true, 0.5, false, false, false);
        let mapped = planet("High metal content body", true, 0.5, false, true, false);
        let mapped_fd = planet("High metal content body", true, 0.5, true, true, true);
        assert!(scan_only < mapped && mapped < mapped_fd);
    }

    #[test]
    fn tiny_icy_body_hits_minimum() {
        assert_eq!(planet("Icy body", false, 0.0001, false, false, false), 500);
    }

    #[test]
    fn white_dwarf_beats_main_sequence() {
        assert!(star_value("DA", 0.6, false) > star_value("K", 0.8, false));
    }
}
