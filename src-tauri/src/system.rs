//! Folds journal events into the state of the commander's current system.

use crate::bio::{self, BioView};
use crate::values::{self, PlanetValueInput};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

// ---- journal event shapes (only the fields we use) ----

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SystemArrival {
    star_system: String,
    system_address: u64,
    #[serde(default)]
    star_pos: [f64; 3],
    #[serde(default)]
    population: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ScanEvent {
    body_name: String,
    #[serde(rename = "BodyID")]
    body_id: u32,
    system_address: Option<u64>,
    #[serde(rename = "DistanceFromArrivalLS", default)]
    distance_from_arrival_ls: f64,
    star_type: Option<String>,
    luminosity: Option<String>,
    stellar_mass: Option<f64>,
    planet_class: Option<String>,
    #[serde(rename = "MassEM")]
    mass_em: Option<f64>,
    landable: Option<bool>,
    terraform_state: Option<String>,
    atmosphere: Option<String>,
    atmosphere_type: Option<String>,
    #[serde(default)]
    atmosphere_composition: Vec<Component>,
    surface_gravity: Option<f64>,
    surface_temperature: Option<f64>,
    surface_pressure: Option<f64>,
    volcanism: Option<String>,
    #[serde(default)]
    materials: Vec<Component>,
    orbital_period: Option<f64>,
    #[serde(default)]
    rings: Vec<RingData>,
    #[serde(default)]
    was_discovered: bool,
    #[serde(default)]
    was_mapped: bool,
    #[serde(default)]
    was_footfalled: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Component {
    name: String,
    percent: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RingData {
    name: String,
    ring_class: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Signal {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Type_Localised")]
    kind_localised: Option<String>,
    count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Genus {
    genus: String,
    #[serde(rename = "Genus_Localised")]
    genus_localised: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BodySignalsEvent {
    body_name: String,
    #[serde(rename = "BodyID")]
    body_id: u32,
    system_address: u64,
    #[serde(default)]
    signals: Vec<Signal>,
    #[serde(default)]
    genuses: Vec<Genus>,
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
    genus: String,
    #[serde(rename = "Genus_Localised")]
    genus_localised: Option<String>,
    species: String,
    #[serde(rename = "Species_Localised")]
    species_localised: Option<String>,
    system_address: u64,
    body: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DisembarkEvent {
    system_address: u64,
    #[serde(rename = "BodyID")]
    body_id: u32,
    #[serde(default)]
    on_planet: bool,
    #[serde(default)]
    taxi: bool,
    #[serde(default)]
    multicrew: bool,
}

// ---- views sent to the UI ----

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum BodyKind {
    Star,
    Planet,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotspot {
    pub kind: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ring {
    pub name: String,
    pub short_name: String,
    pub ring_class: String,
    /// `None` until the ring has been mapped with the DSS.
    pub hotspots: Option<Vec<Hotspot>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeciesProgress {
    pub name: String,
    /// 0 = progress lost, 1 = logged, 2 = sampled, 3 = analysed (complete).
    pub samples: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Body {
    pub id: u32,
    pub name: String,
    pub short_name: String,
    pub kind: BodyKind,
    /// `PlanetClass` for planets, `StarType` for stars.
    pub class: String,
    pub distance_ls: f64,
    pub landable: bool,
    pub terraformable: bool,
    pub atmosphere: Option<String>,
    pub gravity_g: Option<f64>,
    pub temperature_k: Option<f64>,
    pub was_discovered: bool,
    pub was_mapped: bool,
    pub was_footfalled: bool,
    pub mapped: bool,
    pub mapped_efficiently: bool,
    pub footfall: bool,
    pub bio_signals: u32,
    pub geo_signals: u32,
    pub species: Vec<SpeciesProgress>,
    pub bio: Option<BioView>,
    pub rings: Vec<Ring>,
    pub value_scan: u64,
    pub value_mapped: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemView {
    pub name: String,
    pub address: u64,
    pub star_pos: [f64; 3],
    pub body_count: Option<u32>,
    pub bodies_found: u32,
    pub all_found: bool,
    pub bodies: Vec<Body>,
}

// ---- tracker ----

#[derive(Default)]
struct BodySignals {
    bio: u32,
    geo: u32,
    /// (codex id, localised name) as reported by the DSS.
    genuses: Vec<(String, String)>,
}

struct TrackedSpecies {
    genus_id: String,
    genus_name: String,
    species_id: String,
    name: String,
    /// 0 = progress lost, 1 = logged, 2 = sampled, 3 = analysed (complete).
    samples: u8,
}

struct SystemState {
    name: String,
    address: u64,
    star_pos: [f64; 3],
    region: Option<u8>,
    population: u64,
    body_count: Option<u32>,
    all_found: bool,
    scans: BTreeMap<u32, ScanEvent>,
    // These can arrive before the body's Scan event, so they're keyed separately.
    signals: HashMap<u32, BodySignals>,
    ring_hotspots: HashMap<String, Vec<Hotspot>>,
    mapped: HashMap<u32, bool>,
    footfall: HashSet<u32>,
    species: HashMap<u32, Vec<TrackedSpecies>>,
}

impl SystemState {
    fn new(arrival: SystemArrival) -> Self {
        Self {
            name: arrival.star_system,
            address: arrival.system_address,
            star_pos: arrival.star_pos,
            region: bio::region_at(arrival.star_pos),
            population: arrival.population,
            body_count: None,
            all_found: false,
            scans: BTreeMap::new(),
            signals: HashMap::new(),
            ring_hotspots: HashMap::new(),
            mapped: HashMap::new(),
            footfall: HashSet::new(),
            species: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemRef {
    pub name: String,
    pub address: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CarrierState {
    pub location: SystemRef,
    /// A scheduled jump that hasn't happened yet.
    pub pending_jump: Option<SystemRef>,
}

#[derive(Default)]
pub struct Tracker {
    pub commander: Option<String>,
    /// Your own fleet carrier, from `CarrierLocation` and `CarrierJumpRequest`.
    pub carrier: Option<CarrierState>,
    /// Laden jump range of the current ship, from the latest `Loadout`.
    pub jump_range: Option<f64>,
    system: Option<SystemState>,
}

fn system_ref(ev: &Value, name_field: &str) -> Option<SystemRef> {
    Some(SystemRef {
        name: ev.get(name_field)?.as_str()?.to_string(),
        address: ev.get("SystemAddress")?.as_u64()?,
    })
}

fn parse<T: for<'de> Deserialize<'de>>(ev: &Value) -> Option<T> {
    T::deserialize(ev).ok()
}

impl Tracker {
    /// Applies one journal event. Returns true if anything visible changed.
    pub fn apply(&mut self, ev: &Value) -> bool {
        let Some(name) = ev.get("event").and_then(Value::as_str) else {
            return false;
        };
        match name {
            "Commander" | "LoadGame" => {
                let cmdr = ev.get("Name").or_else(|| ev.get("Commander")).and_then(Value::as_str);
                if let Some(cmdr) = cmdr {
                    self.commander = Some(cmdr.to_string());
                }
                true
            }
            "Location" | "FSDJump" | "CarrierJump" => {
                let Some(arrival) = parse::<SystemArrival>(ev) else { return false };
                // Relogging in the same system emits Location again; keep what we already know.
                if self.system.as_ref().is_some_and(|s| s.address == arrival.system_address) {
                    return false;
                }
                self.system = Some(SystemState::new(arrival));
                true
            }
            "Loadout" => {
                let range = ev.get("MaxJumpRange").and_then(Value::as_f64).filter(|r| *r > 0.0);
                let changed = range != self.jump_range;
                self.jump_range = range;
                changed
            }
            "CarrierLocation" => {
                let Some(location) = system_ref(ev, "StarSystem") else { return false };
                let pending_jump = self
                    .carrier
                    .take()
                    .and_then(|c| c.pending_jump)
                    .filter(|p| p.address != location.address);
                self.carrier = Some(CarrierState { location, pending_jump });
                true
            }
            "CarrierJumpRequest" => {
                let (Some(carrier), Some(target)) = (self.carrier.as_mut(), system_ref(ev, "SystemName")) else {
                    return false;
                };
                carrier.pending_jump = Some(target);
                true
            }
            "CarrierJumpCancelled" => self.carrier.as_mut().and_then(|c| c.pending_jump.take()).is_some(),
            _ => self.apply_to_system(name, ev),
        }
    }

    fn apply_to_system(&mut self, name: &str, ev: &Value) -> bool {
        let Some(sys) = self.system.as_mut() else { return false };
        let in_system = |addr: Option<u64>| addr.is_none_or(|a| a == sys.address);

        match name {
            "FSSDiscoveryScan" => {
                if !in_system(ev.get("SystemAddress").and_then(Value::as_u64)) {
                    return false;
                }
                sys.body_count = ev.get("BodyCount").and_then(Value::as_u64).map(|n| n as u32);
                true
            }
            "FSSAllBodiesFound" => {
                if !in_system(ev.get("SystemAddress").and_then(Value::as_u64)) {
                    return false;
                }
                sys.all_found = true;
                true
            }
            "Scan" => {
                let Some(scan) = parse::<ScanEvent>(ev) else { return false };
                // Belt clusters have neither a star type nor a planet class.
                if !in_system(scan.system_address) || (scan.star_type.is_none() && scan.planet_class.is_none()) {
                    return false;
                }
                sys.scans.insert(scan.body_id, scan);
                true
            }
            "FSSBodySignals" | "SAASignalsFound" => {
                let Some(e) = parse::<BodySignalsEvent>(ev) else { return false };
                if e.system_address != sys.address {
                    return false;
                }
                if e.body_name.ends_with(" Ring") {
                    let hotspots = e
                        .signals
                        .into_iter()
                        .map(|s| Hotspot { kind: s.kind_localised.unwrap_or(s.kind), count: s.count })
                        .collect();
                    sys.ring_hotspots.insert(e.body_name, hotspots);
                    return true;
                }
                let entry = sys.signals.entry(e.body_id).or_default();
                for s in &e.signals {
                    if s.kind.contains("Biological") {
                        entry.bio = s.count;
                    } else if s.kind.contains("Geological") {
                        entry.geo = s.count;
                    }
                }
                if !e.genuses.is_empty() {
                    entry.genuses = e
                        .genuses
                        .into_iter()
                        .map(|g| {
                            let name = g.genus_localised.unwrap_or_else(|| g.genus.clone());
                            (g.genus, name)
                        })
                        .collect();
                }
                true
            }
            "SAAScanComplete" => {
                let Some(e) = parse::<MapCompleteEvent>(ev) else { return false };
                if e.system_address != sys.address {
                    return false;
                }
                sys.mapped.insert(e.body_id, e.probes_used <= e.efficiency_target);
                true
            }
            "ScanOrganic" => {
                let Some(e) = parse::<ScanOrganicEvent>(ev) else { return false };
                if e.system_address != sys.address {
                    return false;
                }
                let stage = match e.scan_type.as_str() {
                    "Log" => 1,
                    "Sample" => 2,
                    "Analyse" => 3,
                    _ => return false,
                };
                // Starting a new species abandons any other one that wasn't finished.
                if stage == 1 {
                    for list in sys.species.values_mut() {
                        for sp in list.iter_mut() {
                            if sp.species_id != e.species && (1..3).contains(&sp.samples) {
                                sp.samples = 0;
                            }
                        }
                    }
                }
                let list = sys.species.entry(e.body).or_default();
                match list.iter_mut().find(|s| s.species_id == e.species) {
                    Some(sp) if sp.samples < 3 => sp.samples = stage,
                    Some(_) => {}
                    None => list.push(TrackedSpecies {
                        genus_name: e.genus_localised.unwrap_or_else(|| e.genus.clone()),
                        genus_id: e.genus,
                        name: e.species_localised.unwrap_or_else(|| e.species.clone()),
                        species_id: e.species,
                        samples: stage,
                    }),
                }
                true
            }
            "Disembark" => {
                let Some(e) = parse::<DisembarkEvent>(ev) else { return false };
                if e.system_address != sys.address || !e.on_planet || e.taxi || e.multicrew {
                    return false;
                }
                sys.footfall.insert(e.body_id)
            }
            _ => false,
        }
    }

    /// Name and coordinates of the system you're in, without building the full view.
    pub fn current_system(&self) -> Option<(&str, [f64; 3])> {
        self.system.as_ref().map(|s| (s.name.as_str(), s.star_pos))
    }

    pub fn view(&self) -> Option<SystemView> {
        let sys = self.system.as_ref()?;
        let prefix = format!("{} ", sys.name);
        let short = |name: &str| name.strip_prefix(&prefix).unwrap_or(name).to_string();

        let stars: Vec<bio::Star> = sys
            .scans
            .values()
            .filter_map(|s| {
                Some(bio::Star {
                    short_name: short(&s.body_name),
                    star_type: s.star_type.clone()?,
                    luminosity: s.luminosity.clone().unwrap_or_default(),
                    distance_ls: s.distance_from_arrival_ls,
                })
            })
            .collect();
        let planet_classes: Vec<&str> = sys.scans.values().filter_map(|s| s.planet_class.as_deref()).collect();
        let context = bio::SystemContext {
            name: &sys.name,
            pos: sys.star_pos,
            region: sys.region,
            stars: &stars,
            planet_classes: &planet_classes,
        };

        let bodies: Vec<Body> = sys
            .scans
            .values()
            .map(|s| {
                let signals = sys.signals.get(&s.body_id);
                let mapped = sys.mapped.get(&s.body_id).copied();
                let terraformable = s.terraform_state.as_deref().is_some_and(|t| !t.is_empty());
                let (kind, class, value_scan, value_mapped) = match (&s.star_type, &s.planet_class) {
                    (Some(star), _) => {
                        let v = values::star_value(star, s.stellar_mass.unwrap_or(0.0), !s.was_discovered);
                        (BodyKind::Star, star.clone(), v, v)
                    }
                    (None, Some(pc)) => {
                        let input = |mapped: bool| PlanetValueInput {
                            planet_class: pc,
                            terraformable,
                            earth_masses: s.mass_em.unwrap_or(0.0),
                            first_discovery: !s.was_discovered,
                            mapped,
                            first_mapped: !s.was_mapped,
                            efficient: true,
                        };
                        let scan = values::planet_value(&input(false));
                        let mapped = values::planet_value(&input(true));
                        (BodyKind::Planet, pc.clone(), scan, mapped)
                    }
                    (None, None) => unreachable!("filtered out when the scan was recorded"),
                };
                let tracked = sys.species.get(&s.body_id).map(Vec::as_slice).unwrap_or_default();
                let genuses = signals.map(|x| x.genuses.as_slice()).unwrap_or_default();
                let bio_signals = signals.map_or(0, |x| x.bio);
                let has_bio = bio_signals > 0 || !genuses.is_empty() || !tracked.is_empty();
                let bio = (kind == BodyKind::Planet && has_bio).then(|| {
                    let sampled: Vec<bio::ScannedSpecies> = tracked
                        .iter()
                        .map(|t| bio::ScannedSpecies {
                            genus_id: &t.genus_id,
                            genus_name: &t.genus_name,
                            species_id: &t.species_id,
                            species_name: &t.name,
                            samples: t.samples,
                        })
                        .collect();
                    // Vista Genomics pays the first-logged bonus on species nobody has handed in yet; an unwalked
                    // body in an unpopulated system is the best available sign of that.
                    let first_logged_likely = !s.was_footfalled && sys.population == 0;
                    bio::body_bio(&bio_planet(s, &short), &context, bio_signals, genuses, &sampled, first_logged_likely)
                });
                let rings = s
                    .rings
                    .iter()
                    .map(|r| Ring {
                        name: r.name.clone(),
                        short_name: short(&r.name),
                        ring_class: r.ring_class.trim_start_matches("eRingClass_").to_string(),
                        hotspots: sys.ring_hotspots.get(&r.name).cloned(),
                    })
                    .collect();
                Body {
                    id: s.body_id,
                    name: s.body_name.clone(),
                    short_name: short(&s.body_name),
                    kind,
                    class,
                    distance_ls: s.distance_from_arrival_ls,
                    landable: s.landable.unwrap_or(false),
                    terraformable,
                    atmosphere: s.atmosphere.clone().filter(|a| !a.is_empty()),
                    gravity_g: s.surface_gravity.map(|g| g / 9.80665),
                    temperature_k: s.surface_temperature,
                    was_discovered: s.was_discovered,
                    was_mapped: s.was_mapped,
                    was_footfalled: s.was_footfalled,
                    mapped: mapped.is_some(),
                    mapped_efficiently: mapped.unwrap_or(false),
                    footfall: sys.footfall.contains(&s.body_id),
                    bio_signals,
                    geo_signals: signals.map_or(0, |x| x.geo),
                    species: tracked
                        .iter()
                        .map(|t| SpeciesProgress { name: t.name.clone(), samples: t.samples })
                        .collect(),
                    bio,
                    rings,
                    value_scan,
                    value_mapped,
                }
            })
            .collect();

        Some(SystemView {
            name: sys.name.clone(),
            address: sys.address,
            star_pos: sys.star_pos,
            body_count: sys.body_count,
            bodies_found: bodies.len() as u32,
            all_found: sys.all_found,
            bodies,
        })
    }
}

fn bio_planet(s: &ScanEvent, short: &impl Fn(&str) -> String) -> bio::Planet {
    bio::Planet {
        short_name: short(&s.body_name),
        class: s.planet_class.clone().unwrap_or_default(),
        atmosphere: s.atmosphere_type.clone().unwrap_or_default(),
        gases: s.atmosphere_composition.iter().map(|c| (c.name.clone(), c.percent)).collect(),
        gravity_ms2: s.surface_gravity.unwrap_or(0.0),
        temperature_k: s.surface_temperature,
        pressure_pa: s.surface_pressure,
        volcanism: s.volcanism.clone().unwrap_or_default(),
        materials: s.materials.iter().map(|m| m.name.clone()).collect(),
        orbital_period_s: s.orbital_period.unwrap_or(0.0),
        distance_ls: s.distance_from_arrival_ls,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Tracker {
        let mut t = Tracker::default();
        for l in lines {
            t.apply(&serde_json::from_str(l).unwrap());
        }
        t
    }

    // Lines taken from a real journal, trimmed to the fields that matter.
    const JUMP: &str = r#"{ "event":"FSDJump", "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "StarPos":[-7583.15625,337.87500,3297.78125] }"#;
    const STAR: &str = r#"{ "event":"Scan", "ScanType":"AutoScan", "BodyName":"Smojeia SE-Q d5-3", "BodyID":0, "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "DistanceFromArrivalLS":0.0, "StarType":"F", "StellarMass":1.2, "WasDiscovered":false, "WasMapped":false, "WasFootfalled":false }"#;
    const PLANET: &str = r#"{ "event":"Scan", "ScanType":"Detailed", "BodyName":"Smojeia SE-Q d5-3 1", "BodyID":12, "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "DistanceFromArrivalLS":512.3, "TerraformState":"Terraformable", "PlanetClass":"High metal content body", "Atmosphere":"", "MassEM":0.5, "Landable":false, "Rings":[ { "Name":"Smojeia SE-Q d5-3 1 A Ring", "RingClass":"eRingClass_Metalic", "MassMT":1.0, "InnerRad":1.0, "OuterRad":2.0 }, { "Name":"Smojeia SE-Q d5-3 1 B Ring", "RingClass":"eRingClass_MetalRich", "MassMT":1.0, "InnerRad":1.0, "OuterRad":2.0 } ], "WasDiscovered":false, "WasMapped":false, "WasFootfalled":false }"#;
    const RING_HOTSPOTS: &str = r#"{ "event":"SAASignalsFound", "BodyName":"Smojeia SE-Q d5-3 1 B Ring", "SystemAddress":111979596467, "BodyID":15, "Signals":[ { "Type":"Serendibite", "Count":4 }, { "Type":"Painite", "Count":6 } ], "Genuses":[] }"#;

    #[test]
    fn tracks_bodies_rings_and_hotspots() {
        let t = feed(&[JUMP, STAR, PLANET, RING_HOTSPOTS]);
        let v = t.view().unwrap();
        assert_eq!(v.name, "Smojeia SE-Q d5-3");
        assert_eq!(v.bodies_found, 2);
        let planet = v.bodies.iter().find(|b| b.id == 12).unwrap();
        assert_eq!(planet.short_name, "1");
        assert!(planet.terraformable && !planet.was_discovered);
        assert!(planet.value_mapped > planet.value_scan);
        assert!(planet.rings[0].hotspots.is_none());
        let hs = planet.rings[1].hotspots.as_ref().unwrap();
        assert_eq!(hs.iter().map(|h| h.count).sum::<u32>(), 10);
    }

    #[test]
    fn signals_before_scan_are_kept_and_mapping_is_tracked() {
        let bio = r#"{ "event":"FSSBodySignals", "BodyName":"Smojeia SE-Q d5-3 1", "BodyID":12, "SystemAddress":111979596467, "Signals":[ { "Type":"$SAA_SignalType_Biological;", "Type_Localised":"Biological", "Count":3 } ] }"#;
        let map = r#"{ "event":"SAAScanComplete", "BodyName":"Smojeia SE-Q d5-3 1", "SystemAddress":111979596467, "BodyID":12, "ProbesUsed":5, "EfficiencyTarget":6 }"#;
        let t = feed(&[JUMP, bio, PLANET, map]);
        let planet = t.view().unwrap().bodies.into_iter().find(|b| b.id == 12).unwrap();
        assert_eq!(planet.bio_signals, 3);
        assert!(planet.mapped && planet.mapped_efficiently);
    }

    #[test]
    fn new_species_abandons_unfinished_one() {
        let organic = |kind: &str, species: &str| {
            format!(r#"{{ "event":"ScanOrganic", "ScanType":"{kind}", "Genus":"g", "Species":"{species}", "Species_Localised":"{species}", "SystemAddress":111979596467, "Body":12 }}"#)
        };
        let lines = [
            JUMP.to_string(),
            PLANET.to_string(),
            organic("Log", "Frutexa Acus"),
            organic("Sample", "Frutexa Acus"),
            organic("Log", "Bacterium Aurasus"),
        ];
        let t = feed(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let planet = t.view().unwrap().bodies.into_iter().find(|b| b.id == 12).unwrap();
        let get = |n: &str| planet.species.iter().find(|s| s.name == n).unwrap().samples;
        assert_eq!(get("Frutexa Acus"), 0);
        assert_eq!(get("Bacterium Aurasus"), 1);
    }

    #[test]
    fn tracks_carrier_jumps_and_jump_range() {
        let t = feed(&[
            r#"{ "event":"Loadout", "Ship":"mandalay", "MaxJumpRange":86.005463 }"#,
            r#"{ "event":"CarrierLocation", "CarrierType":"FleetCarrier", "StarSystem":"Smojeia XK-O d6-21", "SystemAddress":730471664315, "BodyID":0 }"#,
            r#"{ "event":"CarrierJumpRequest", "SystemName":"Byoi Thaa XW-L c8-3", "SystemAddress":895467427146, "DepartureTime":"2026-09-21T16:10:00Z" }"#,
        ]);
        assert_eq!(t.jump_range, Some(86.005463));
        let carrier = t.carrier.as_ref().unwrap();
        assert_eq!(carrier.location.name, "Smojeia XK-O d6-21");
        assert_eq!(carrier.pending_jump.as_ref().unwrap().address, 895467427146);

        let mut t = t;
        t.apply(&serde_json::from_str(r#"{ "event":"CarrierLocation", "StarSystem":"Byoi Thaa XW-L c8-3", "SystemAddress":895467427146 }"#).unwrap());
        let carrier = t.carrier.as_ref().unwrap();
        assert_eq!(carrier.location.address, 895467427146);
        assert!(carrier.pending_jump.is_none());
    }

    #[test]
    fn relog_in_same_system_keeps_scans() {
        let location = r#"{ "event":"Location", "StarSystem":"Smojeia SE-Q d5-3", "SystemAddress":111979596467, "StarPos":[-7583.15625,337.87500,3297.78125] }"#;
        let t = feed(&[JUMP, STAR, location]);
        assert_eq!(t.view().unwrap().bodies_found, 1);
    }
}
