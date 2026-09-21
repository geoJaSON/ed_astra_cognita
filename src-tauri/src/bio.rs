//! Exobiology species prediction, ported from EDMC-BioScan's rule engine (GPL-2.0).
//! Rules and reference data are generated into `data/bio/` by `tools/import_bio_data.py`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

// BioScan's unit conversions; not the standard g and atm, but kept so the rule thresholds behave identically.
const GRAVITY_G: f64 = 9.797759;
const PRESSURE_ATM: f64 = 101231.65625;
const NEBULA_RANGE_LY: f64 = 150.0;
const PLANETARY_NEBULA_RANGE_LY: f64 = 100.0;
/// Base value plus the 4x first-logged bonus, confirmed against Vista Genomics sales in the journal.
pub const FIRST_LOGGED_MULTIPLIER: u64 = 5;

// ---- inputs ----

pub struct Star {
    /// Body name without the system prefix; a lone primary star keeps the system name.
    pub short_name: String,
    pub star_type: String,
    pub luminosity: String,
    pub distance_ls: f64,
}

pub struct Planet {
    pub short_name: String,
    pub class: String,
    /// Journal `AtmosphereType`, e.g. "CarbonDioxide" or "None".
    pub atmosphere: String,
    pub gases: Vec<(String, f64)>,
    pub gravity_ms2: f64,
    pub temperature_k: Option<f64>,
    pub pressure_pa: Option<f64>,
    pub volcanism: String,
    pub materials: Vec<String>,
    pub orbital_period_s: f64,
    pub distance_ls: f64,
}

pub struct SystemContext<'a> {
    pub name: &'a str,
    pub pos: [f64; 3],
    pub region: Option<u8>,
    pub stars: &'a [Star],
    pub planet_classes: &'a [&'a str],
}

impl SystemContext<'_> {
    fn star(&self, short_name: &str) -> Option<&Star> {
        self.stars.iter().find(|s| s.short_name == short_name)
    }

    fn main_star(&self) -> Option<&Star> {
        self.stars.iter().find(|s| s.distance_ls == 0.0)
    }
}

pub struct ScannedSpecies<'a> {
    pub genus_id: &'a str,
    pub genus_name: &'a str,
    pub species_id: &'a str,
    pub species_name: &'a str,
    pub samples: u8,
}

// ---- outputs ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub name: String,
    /// Payout including the first-logged bonus when it applies.
    pub value: u64,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sampled {
    pub species: String,
    pub samples: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenusView {
    pub name: String,
    pub colony_distance_m: Option<u32>,
    /// Reported by the DSS or seen while sampling; otherwise only predicted.
    pub confirmed: bool,
    pub candidates: Vec<Candidate>,
    pub sampled: Option<Sampled>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BioView {
    pub signals: u32,
    /// True once the DSS has reported which genera are present.
    pub genera_known: bool,
    pub genera: Vec<GenusView>,
    pub value_min: u64,
    pub value_max: u64,
    /// False when a signal couldn't be matched to any predicted species, so the range may be low.
    pub value_complete: bool,
    pub multiplier: u64,
}

// ---- reference data ----

enum Rule {
    /// `None` means any atmosphere at all.
    Atmosphere(Option<Vec<String>>),
    AtmosphereComponent(Vec<(String, f64)>),
    MinGravity(f64),
    MaxGravity(f64),
    MinTemperature(f64),
    MaxTemperature(f64),
    MinPressure(f64),
    MaxPressure(f64),
    MaxOrbitalPeriod(f64),
    Volcanism(Volcanism),
    BodyType(Vec<String>),
    Regions(Vec<String>),
    Guardian(bool),
    /// `None` means any tuber zone.
    Tuber(Option<Vec<String>>),
    Bodies(Vec<String>),
    ParentStar(Vec<String>),
    Star(Vec<StarRequirement>),
    Nebula { include_planetary: bool },
    MinDistance(f64),
    System(String),
}

enum Volcanism {
    Any,
    None,
    /// Some volcanism, but not this kind.
    Not(String),
    /// Substring match, or exact match for entries starting with '='.
    OneOf(Vec<String>),
}

struct StarRequirement {
    class: String,
    luminosity: Option<String>,
}

struct Species {
    id: String,
    name: String,
    value: u64,
    rulesets: Vec<Vec<Rule>>,
}

enum ColorSource {
    Star(BTreeMap<String, String>),
    Element(BTreeMap<String, String>),
}

enum GenusColors {
    None,
    /// One mapping for the whole genus.
    Star(BTreeMap<String, String>),
    PerSpecies(HashMap<String, ColorSource>),
}

struct Genus {
    id: String,
    name: String,
    colony_distance_m: u32,
    colors: GenusColors,
    species: Vec<Species>,
}

struct Data {
    genera: Vec<Genus>,
    region_map: Vec<Vec<(u32, u8)>>,
    region_groups: HashMap<String, Vec<u8>>,
    guardian_nebulae: Vec<(f64, [f64; 3])>,
    tuber_zones: Vec<(String, (f64, f64), [f64; 3])>,
    nebulae_large: Vec<[f64; 3]>,
    nebulae_planetary: Vec<[f64; 3]>,
    nebula_sectors: Vec<String>,
}

#[derive(Deserialize)]
struct SpeciesFile {
    genera: BTreeMap<String, RawGenus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawGenus {
    name: String,
    colony_distance: u32,
    colors: Option<Map<String, Value>>,
    species: BTreeMap<String, RawSpecies>,
}

#[derive(Deserialize)]
struct RawSpecies {
    name: String,
    value: u64,
    rulesets: Vec<Map<String, Value>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegionsFile {
    map: Vec<Vec<(u32, u8)>>,
    groups: HashMap<String, Vec<u8>>,
    guardian_nebulae: HashMap<String, (f64, [f64; 3])>,
    tuber_zones: HashMap<String, ((f64, f64), [f64; 3])>,
}

#[derive(Deserialize)]
struct NebulaeFile {
    large: Vec<[f64; 3]>,
    planetary: Vec<[f64; 3]>,
    sectors: Vec<String>,
}

fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        let species: SpeciesFile =
            serde_json::from_str(include_str!("../data/bio/species.json")).expect("data/bio/species.json is malformed");
        let regions: RegionsFile =
            serde_json::from_str(include_str!("../data/bio/regions.json")).expect("data/bio/regions.json is malformed");
        let nebulae: NebulaeFile =
            serde_json::from_str(include_str!("../data/bio/nebulae.json")).expect("data/bio/nebulae.json is malformed");
        Data {
            genera: species.genera.into_iter().map(|(id, g)| parse_genus(id, g)).collect(),
            region_map: regions.map,
            region_groups: regions.groups,
            guardian_nebulae: regions.guardian_nebulae.into_values().collect(),
            tuber_zones: regions.tuber_zones.into_iter().map(|(name, (range, pos))| (name, range, pos)).collect(),
            nebulae_large: nebulae.large,
            nebulae_planetary: nebulae.planetary,
            nebula_sectors: nebulae.sectors,
        }
    })
}

fn string_list(key: &str, v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items
            .iter()
            .map(|i| i.as_str().unwrap_or_else(|| panic!("{key}: expected strings, got {v}")).to_string())
            .collect(),
        _ => panic!("{key}: expected string or list, got {v}"),
    }
}

fn number(key: &str, v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("{key}: expected number, got {v}"))
}

/// Returns `None` for keys BioScan itself ignores (such as the singular `region`).
fn parse_rule(key: &str, v: &Value) -> Option<Rule> {
    Some(match key {
        "atmosphere" => Rule::Atmosphere((v != "Any").then(|| string_list(key, v))),
        "atmosphere_component" => Rule::AtmosphereComponent(
            v.as_object()
                .unwrap_or_else(|| panic!("{key}: expected object"))
                .iter()
                .map(|(gas, pct)| (gas.clone(), number(key, pct)))
                .collect(),
        ),
        "min_gravity" => Rule::MinGravity(number(key, v)),
        "max_gravity" => Rule::MaxGravity(number(key, v)),
        "min_temperature" => Rule::MinTemperature(number(key, v)),
        "max_temperature" => Rule::MaxTemperature(number(key, v)),
        "min_pressure" => Rule::MinPressure(number(key, v)),
        "max_pressure" => Rule::MaxPressure(number(key, v)),
        "max_orbital_period" => Rule::MaxOrbitalPeriod(number(key, v)),
        "volcanism" => Rule::Volcanism(match v {
            Value::String(s) if s == "Any" => Volcanism::Any,
            Value::String(s) if s == "None" => Volcanism::None,
            Value::String(s) if s.starts_with('!') => Volcanism::Not(s[1..].to_string()),
            Value::Array(_) => Volcanism::OneOf(string_list(key, v)),
            _ => panic!("{key}: unsupported value {v}"),
        }),
        "body_type" => Rule::BodyType(string_list(key, v)),
        "regions" => Rule::Regions(string_list(key, v)),
        "guardian" => Rule::Guardian(v.as_bool().unwrap_or_else(|| panic!("{key}: expected bool"))),
        "tuber" => Rule::Tuber((v != "Any").then(|| string_list(key, v))),
        "bodies" => Rule::Bodies(string_list(key, v)),
        "parent_star" => Rule::ParentStar(string_list(key, v)),
        // BioScan only honours tuple entries as (class, luminosity); list entries are treated the same here,
        // which is what the data clearly intends.
        "star" => Rule::Star(match v {
            Value::String(s) => vec![StarRequirement { class: s.clone(), luminosity: None }],
            Value::Array(items) => items
                .iter()
                .map(|i| match i {
                    Value::String(s) => StarRequirement { class: s.clone(), luminosity: None },
                    Value::Array(pair) if pair.len() == 2 => StarRequirement {
                        class: pair[0].as_str().expect("star class").to_string(),
                        luminosity: Some(pair[1].as_str().expect("star luminosity").to_string()),
                    },
                    _ => panic!("{key}: unsupported entry {i}"),
                })
                .collect(),
            _ => panic!("{key}: unsupported value {v}"),
        }),
        "nebula" => Rule::Nebula { include_planetary: v == "all" },
        "distance" => Rule::MinDistance(number(key, v)),
        "system" => Rule::System(v.as_str().expect("system name").to_string()),
        "region" => return None,
        _ => panic!("unknown bio rule '{key}'"),
    })
}

fn parse_color_map(v: &Value) -> BTreeMap<String, String> {
    v.as_object()
        .expect("color map")
        .iter()
        .map(|(k, c)| (k.clone(), c.as_str().expect("color name").to_string()))
        .collect()
}

fn parse_color_source(v: &Value) -> ColorSource {
    let obj = v.as_object().expect("species colors");
    match (obj.get("star"), obj.get("element")) {
        (Some(m), None) => ColorSource::Star(parse_color_map(m)),
        (None, Some(m)) => ColorSource::Element(parse_color_map(m)),
        _ => panic!("species colors need exactly one of star/element: {v}"),
    }
}

fn parse_genus(id: String, raw: RawGenus) -> Genus {
    let colors = match raw.colors {
        None => GenusColors::None,
        Some(c) => match (c.get("star"), c.get("species")) {
            (Some(m), None) => GenusColors::Star(parse_color_map(m)),
            (None, Some(per)) => GenusColors::PerSpecies(
                per.as_object().expect("per-species colors").iter().map(|(k, v)| (k.clone(), parse_color_source(v))).collect(),
            ),
            _ => panic!("genus {id} colors need exactly one of star/species"),
        },
    };
    let species = raw
        .species
        .into_iter()
        .map(|(sid, s)| Species {
            id: sid,
            name: s.name,
            value: s.value,
            rulesets: s
                .rulesets
                .iter()
                .map(|rs| rs.iter().filter_map(|(k, v)| parse_rule(k, v)).collect())
                .collect(),
        })
        .collect();
    Genus { id, name: raw.name, colony_distance_m: raw.colony_distance, colors, species }
}

// ---- evaluation ----

const REGION_X0: f64 = -49985.0;
const REGION_Z0: f64 = -24105.0;

/// Codex region id (1-42) for galactic coordinates, from klightspeed's region map.
pub fn region_at(pos: [f64; 3]) -> Option<u8> {
    let px = ((pos[0] - REGION_X0) * 83.0 / 4096.0) as i64;
    let pz = ((pos[2] - REGION_Z0) * 83.0 / 4096.0) as i64;
    let row = data().region_map.get(usize::try_from(pz).ok()?)?;
    if px < 0 {
        return None;
    }
    let mut x = 0i64;
    for &(run, id) in row {
        x += run as i64;
        if px < x {
            return (id != 0).then_some(id);
        }
    }
    None
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Whether a journal star type matches a rule's simple class (giants and white dwarf subtypes included).
fn star_matches(class: &str, star_type: &str) -> bool {
    match class {
        "A" => matches!(star_type, "A" | "A_BlueWhiteSuperGiant"),
        "B" => matches!(star_type, "B" | "B_BlueWhiteSuperGiant"),
        "F" => matches!(star_type, "F" | "F_WhiteSuperGiant"),
        "G" => matches!(star_type, "G" | "G_WhiteSuperGiant"),
        "K" => matches!(star_type, "K" | "K_OrangeGiant"),
        "M" => matches!(star_type, "M" | "M_RedGiant" | "M_RedSuperGiant"),
        "D" | "C" | "W" => star_type.starts_with(class),
        _ => star_type == class,
    }
}

/// Stars the planet orbits, from its name: "AB 1 a" orbits A and B, anything else the primary.
fn parent_stars(planet: &Planet, system_name: &str) -> Vec<String> {
    match planet.short_name.split_once(' ') {
        Some((prefix, rest)) if !prefix.is_empty() && !rest.is_empty() && prefix.bytes().all(|b| b.is_ascii_uppercase()) => {
            prefix.chars().map(String::from).collect()
        }
        _ => vec![system_name.to_string()],
    }
}

/// Bio colours normally follow the parent star, but around a black hole the orbiting stars can set it instead.
fn star_orbits_black_hole(star: &Star, planet: &Planet, sys: &SystemContext) -> bool {
    if star.short_name == sys.name || !planet.short_name.starts_with(&format!("{} ", star.short_name)) {
        return false;
    }
    let is_hole = |name: &str| sys.star(name).is_some_and(|s| s.star_type == "H");
    let primary_is_system_star = sys.main_star().is_some_and(|m| m.short_name == sys.name);
    if primary_is_system_star {
        return sys.main_star().is_some_and(|m| m.star_type == "H");
    }
    let parts: Vec<&str> = star.short_name.split(' ').collect();
    if parts[0].len() > 1 {
        parts[0].chars().any(|c| is_hole(&c.to_string()))
    } else if parts.len() == 1 {
        star.star_type == "H"
    } else {
        is_hole(parts[0])
    }
}

/// Colours the planet's stars allow, or `None` when no star that could colour it has been scanned (for example
/// after relogging on a planet whose system arrival is older than the startup replay), so nothing is ruled out.
fn star_colors(map: &BTreeMap<String, String>, planet: &Planet, sys: &SystemContext) -> Option<BTreeSet<String>> {
    let parents = parent_stars(planet, sys.name);
    if sys.main_star().is_none() && !parents.iter().any(|p| sys.star(p).is_some()) {
        return None;
    }
    let mut colors = BTreeSet::new();
    'parents: for parent in &parents {
        if let Some(star) = sys.star(parent) {
            for (class, color) in map {
                if star_matches(class, &star.star_type) {
                    colors.insert(color.clone());
                    break 'parents;
                }
            }
        }
    }
    for star in sys.stars.iter().filter(|s| !parents.contains(&s.short_name)) {
        if star.distance_ls == 0.0 || star_orbits_black_hole(star, planet, sys) {
            if let Some((_, color)) = map.iter().find(|(class, _)| star_matches(class, &star.star_type)) {
                colors.insert(color.clone());
            }
        }
    }
    Some(colors)
}

fn rule_passes(rule: &Rule, p: &Planet, sys: &SystemContext) -> bool {
    let d = data();
    let nonzero = |v: Option<f64>| v.filter(|x| *x != 0.0);
    match rule {
        Rule::Atmosphere(None) => !(p.atmosphere.is_empty() || p.atmosphere == "None"),
        Rule::Atmosphere(Some(list)) => list.contains(&p.atmosphere),
        Rule::AtmosphereComponent(gases) => gases.iter().all(|(gas, min)| {
            p.gases.iter().find(|(g, _)| g == gas).map_or(0.0, |(_, pct)| *pct) >= *min
        }),
        Rule::MinGravity(v) => p.gravity_ms2 / GRAVITY_G >= *v,
        Rule::MaxGravity(v) => p.gravity_ms2 / GRAVITY_G <= *v,
        // Unknown temperature or pressure doesn't rule anything out.
        Rule::MinTemperature(v) => nonzero(p.temperature_k).is_none_or(|t| t >= *v),
        Rule::MaxTemperature(v) => nonzero(p.temperature_k).is_none_or(|t| t <= *v),
        Rule::MinPressure(v) => nonzero(p.pressure_pa).is_none_or(|pr| pr / PRESSURE_ATM >= *v),
        Rule::MaxPressure(v) => nonzero(p.pressure_pa).is_none_or(|pr| pr / PRESSURE_ATM < *v),
        Rule::MaxOrbitalPeriod(v) => p.orbital_period_s < *v,
        Rule::Volcanism(Volcanism::Any) => !p.volcanism.is_empty(),
        Rule::Volcanism(Volcanism::None) => p.volcanism.is_empty(),
        Rule::Volcanism(Volcanism::Not(kind)) => !p.volcanism.is_empty() && !p.volcanism.contains(kind.as_str()),
        Rule::Volcanism(Volcanism::OneOf(kinds)) => kinds.iter().any(|k| match k.strip_prefix('=') {
            Some(exact) => p.volcanism == exact,
            None => p.volcanism.contains(k.as_str()),
        }),
        Rule::BodyType(classes) => classes.contains(&p.class),
        Rule::Regions(names) => {
            let Some(region) = sys.region else { return true };
            let in_group = |name: &str| d.region_groups.get(name).is_some_and(|ids| ids.contains(&region));
            let (excluded, required): (Vec<&String>, Vec<&String>) = names.iter().partition(|n| n.starts_with('!'));
            if excluded.iter().any(|n| in_group(&n[1..])) {
                return false;
            }
            required.is_empty() || required.iter().any(|n| in_group(n))
        }
        Rule::Guardian(required) => {
            !required || d.guardian_nebulae.iter().any(|(range, pos)| distance(sys.pos, *pos) < *range)
        }
        Rule::Tuber(zones) => d.tuber_zones.iter().any(|(name, (min, max), pos)| {
            zones.as_ref().is_none_or(|z| z.contains(name)) && (*min..=*max).contains(&distance(sys.pos, *pos))
        }),
        Rule::Bodies(classes) => sys.planet_classes.iter().any(|c| classes.iter().any(|x| x == c)),
        Rule::ParentStar(classes) => {
            let matches = |star_type: &str| classes.iter().any(|c| star_matches(c, star_type));
            sys.main_star().is_some_and(|m| matches(&m.star_type))
                || parent_stars(p, sys.name).iter().filter_map(|n| sys.star(n)).any(|s| matches(&s.star_type))
        }
        Rule::Star(reqs) => sys.stars.iter().any(|s| {
            reqs.iter().any(|r| {
                star_matches(&r.class, &s.star_type)
                    && r.luminosity.as_ref().is_none_or(|lum| {
                        ["", "a", "b", "ab", "z"].iter().any(|suffix| s.luminosity == format!("{lum}{suffix}"))
                    })
            })
        }),
        Rule::Nebula { include_planetary } => {
            d.nebula_sectors.iter().any(|sector| sys.name.starts_with(sector.as_str()))
                || d.nebulae_large.iter().any(|n| distance(sys.pos, *n) < NEBULA_RANGE_LY)
                || (*include_planetary
                    && d.nebulae_planetary.iter().any(|n| distance(sys.pos, *n) < PLANETARY_NEBULA_RANGE_LY))
        }
        Rule::MinDistance(v) => p.distance_ls >= *v,
        Rule::System(name) => sys.name == name,
    }
}

/// Species of this genus that could grow on the planet, cheapest first, with their possible colours.
fn predict_genus(genus: &Genus, p: &Planet, sys: &SystemContext, multiplier: u64) -> Vec<Candidate> {
    let mut possible: Vec<(&Species, BTreeSet<String>)> = genus
        .species
        .iter()
        .filter(|s| s.rulesets.iter().any(|rs| rs.iter().all(|r| rule_passes(r, p, sys))))
        .map(|s| (s, BTreeSet::new()))
        .collect();

    match &genus.colors {
        GenusColors::None => {}
        GenusColors::Star(map) => {
            if let Some(colors) = star_colors(map, p, sys) {
                if colors.is_empty() {
                    possible.clear();
                }
                for (_, c) in possible.iter_mut() {
                    c.clone_from(&colors);
                }
            }
        }
        GenusColors::PerSpecies(per) => {
            possible.retain_mut(|(species, colors)| {
                *colors = match per.get(&species.id) {
                    Some(ColorSource::Star(map)) => match star_colors(map, p, sys) {
                        Some(found) => found,
                        None => return true,
                    },
                    // Surface materials come with a detailed scan; without them there's nothing to check.
                    Some(ColorSource::Element(_)) if p.materials.is_empty() => return true,
                    Some(ColorSource::Element(map)) => {
                        map.iter().filter(|(el, _)| p.materials.contains(el)).map(|(_, c)| c.clone()).collect()
                    }
                    None => return true,
                };
                !colors.is_empty()
            });
        }
    }

    possible.sort_by_key(|(s, _)| s.value);
    possible
        .into_iter()
        .map(|(s, colors)| Candidate {
            id: s.id.clone(),
            name: s.name.clone(),
            value: s.value * multiplier,
            colors: colors.into_iter().collect(),
        })
        .collect()
}

pub(crate) fn species_value(species_id: &str) -> Option<u64> {
    data().genera.iter().flat_map(|g| &g.species).find(|s| s.id == species_id).map(|s| s.value)
}

fn value_range(candidates: &[Candidate]) -> Option<(u64, u64)> {
    Some((candidates.first()?.value, candidates.last()?.value))
}

/// Builds the exobiology picture for one planet from what's known so far: the signal count,
/// genera reported by the DSS (`(id, localised name)`), and species sampled on foot.
pub fn body_bio(
    p: &Planet,
    sys: &SystemContext,
    signals: u32,
    dss_genera: &[(String, String)],
    sampled: &[ScannedSpecies],
    first_logged_likely: bool,
) -> BioView {
    let d = data();
    let multiplier = if first_logged_likely { FIRST_LOGGED_MULTIPLIER } else { 1 };
    let genus_by_id = |id: &str| d.genera.iter().find(|g| g.id == id);

    // Genera known to be here, keyed by display name (Stratum has two codex ids).
    let mut known: BTreeMap<String, GenusView> = BTreeMap::new();
    for (id, localised) in dss_genera {
        let genus = genus_by_id(id);
        let name = genus.map_or_else(|| localised.clone(), |g| g.name.clone());
        known.entry(name.clone()).or_insert_with(|| GenusView {
            name,
            colony_distance_m: genus.map(|g| g.colony_distance_m),
            confirmed: true,
            candidates: genus.map(|g| predict_genus(g, p, sys, multiplier)).unwrap_or_default(),
            sampled: None,
        });
    }
    for s in sampled {
        let genus = genus_by_id(s.genus_id);
        let name = genus.map_or_else(|| s.genus_name.to_string(), |g| g.name.clone());
        let view = known.entry(name.clone()).or_insert_with(|| GenusView {
            name,
            colony_distance_m: genus.map(|g| g.colony_distance_m),
            confirmed: true,
            candidates: genus.map(|g| predict_genus(g, p, sys, multiplier)).unwrap_or_default(),
            sampled: None,
        });
        // A lost sample (0) doesn't displace a species already completed on this body.
        if view.sampled.as_ref().is_none_or(|v| s.samples >= v.samples) {
            view.sampled = Some(Sampled { species: s.species_name.to_string(), samples: s.samples });
        }
    }

    let mut value_min = 0;
    let mut value_max = 0;
    let mut complete = true;
    for view in known.values() {
        let sampled_value = view.sampled.as_ref().and_then(|v| {
            let id = sampled.iter().find(|s| s.species_name == v.species)?.species_id;
            species_value(id).map(|value| value * multiplier)
        });
        match sampled_value.map(|v| (v, v)).or_else(|| value_range(&view.candidates)) {
            Some((lo, hi)) => {
                value_min += lo;
                value_max += hi;
            }
            None => complete = false,
        }
    }

    // Signals not yet tied to a genus: any genus the rules allow could fill them.
    let unknown = signals.saturating_sub(known.len() as u32) as usize;
    let mut possible: BTreeMap<String, GenusView> = BTreeMap::new();
    if unknown > 0 {
        for genus in &d.genera {
            if known.contains_key(&genus.name) {
                continue;
            }
            let candidates = predict_genus(genus, p, sys, multiplier);
            if candidates.is_empty() {
                continue;
            }
            let view = possible.entry(genus.name.clone()).or_insert_with(|| GenusView {
                name: genus.name.clone(),
                colony_distance_m: Some(genus.colony_distance_m),
                confirmed: false,
                candidates: Vec::new(),
                sampled: None,
            });
            for c in candidates {
                if !view.candidates.iter().any(|x| x.id == c.id) {
                    view.candidates.push(c);
                }
            }
            view.candidates.sort_by_key(|c| c.value);
        }
        let mut mins: Vec<u64> = possible.values().filter_map(|g| value_range(&g.candidates)).map(|r| r.0).collect();
        let mut maxes: Vec<u64> = possible.values().filter_map(|g| value_range(&g.candidates)).map(|r| r.1).collect();
        mins.sort_unstable();
        maxes.sort_unstable_by(|a, b| b.cmp(a));
        value_min += mins.iter().take(unknown).sum::<u64>();
        value_max += maxes.iter().take(unknown).sum::<u64>();
        complete &= possible.len() >= unknown;
    }

    let mut genera: Vec<GenusView> = known.into_values().chain(possible.into_values()).collect();
    genera.sort_by_key(|g| {
        let top = g.candidates.last().map_or(0, |c| c.value);
        (!g.confirmed, std::cmp::Reverse(top))
    });

    BioView {
        signals,
        genera_known: !dss_genera.is_empty(),
        genera,
        value_min,
        value_max,
        value_complete: complete,
        multiplier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_loads_and_every_region_group_exists() {
        let d = data();
        assert!(d.genera.len() >= 20);
        // A species with no rulesets is a lookup-only alias (BioScan files Stratum Araneamus under its species id
        // too, since the journal sometimes reports that as the genus); it is never predicted.
        assert!(d.genera.iter().flat_map(|g| &g.species).all(|s| s.value > 0));
        assert!(d.genera.iter().flat_map(|g| &g.species).filter(|s| s.rulesets.is_empty()).count() <= 1);
        for species in d.genera.iter().flat_map(|g| &g.species) {
            for rule in species.rulesets.iter().flatten() {
                if let Rule::Regions(names) = rule {
                    for n in names {
                        let n = n.trim_start_matches('!');
                        assert!(d.region_groups.contains_key(n), "{}: unknown region group {n}", species.name);
                    }
                }
            }
        }
    }

    #[test]
    fn region_lookup_matches_codex() {
        assert_eq!(region_at([0.0, 0.0, 0.0]), Some(18)); // Sol: Inner Orion Spur
        assert_eq!(region_at([25.21875, -20.90625, 25899.96875]), Some(1)); // Sagittarius A*: Galactic Centre
        // Praea Euq RT-X c28-6, which the journal's CodexEntry places in the Inner Orion Spur.
        assert_eq!(region_at([1026.84375, 69.53125, 1480.9375]), Some(18));
    }

    fn rocky_co2_planet() -> Planet {
        Planet {
            short_name: "1 a".into(),
            class: "Rocky body".into(),
            atmosphere: "CarbonDioxide".into(),
            gases: vec![("CarbonDioxide".into(), 99.0)],
            gravity_ms2: 0.08 * GRAVITY_G,
            temperature_k: Some(180.0),
            pressure_pa: Some(0.01 * PRESSURE_ATM),
            volcanism: String::new(),
            materials: vec!["iron".into(), "polonium".into()],
            orbital_period_s: 1e6,
            distance_ls: 500.0,
        }
    }

    #[test]
    fn predicts_tubus_by_region_and_applies_bonus() {
        let stars = [Star { short_name: "Test".into(), star_type: "K".into(), luminosity: "V".into(), distance_ls: 0.0 }];
        let planet = rocky_co2_planet();
        // In the Inner Orion Spur, Tubus Compagibus can grow but Tubus Cavas (Scutum-Centaurus only) can't.
        let sol = SystemContext { name: "Test", pos: [0.0; 3], region: region_at([0.0; 3]), stars: &stars, planet_classes: &[] };
        let tubus = data().genera.iter().find(|g| g.name == "Tubus").unwrap();
        let names: Vec<String> = predict_genus(tubus, &planet, &sol, 1).into_iter().map(|c| c.name).collect();
        assert!(names.contains(&"Tubus Compagibus".to_string()), "{names:?}");
        assert!(!names.contains(&"Tubus Cavas".to_string()), "{names:?}");

        let bio = body_bio(&planet, &sol, 1, &[], &[], true);
        assert_eq!(bio.multiplier, 5);
        assert!(!bio.genera_known && bio.value_min > 0 && bio.value_min <= bio.value_max);
        assert!(bio.genera.iter().all(|g| !g.confirmed && !g.candidates.is_empty()));
    }

    #[test]
    fn unscanned_star_does_not_rule_out_everything() {
        let sys = SystemContext { name: "Test", pos: [0.0; 3], region: Some(18), stars: &[], planet_classes: &[] };
        let bio = body_bio(&rocky_co2_planet(), &sys, 1, &[], &[], false);
        assert!(bio.genera.iter().any(|g| g.name == "Bacterium" && !g.candidates.is_empty()));
    }

    #[test]
    fn sampled_species_fixes_the_value() {
        let stars = [Star { short_name: "Test".into(), star_type: "K".into(), luminosity: "V".into(), distance_ls: 0.0 }];
        let sys = SystemContext { name: "Test", pos: [0.0; 3], region: Some(18), stars: &stars, planet_classes: &[] };
        let sampled = [ScannedSpecies {
            genus_id: "$Codex_Ent_Bacterial_Genus_Name;",
            genus_name: "Bacterium",
            species_id: "$Codex_Ent_Bacterial_01_Name;",
            species_name: "Bacterium Aurasus",
            samples: 3,
        }];
        let bio = body_bio(&rocky_co2_planet(), &sys, 1, &[], &sampled, false);
        assert_eq!((bio.value_min, bio.value_max), (1_000_000, 1_000_000));
        assert_eq!(bio.genera.len(), 1);
    }
}
