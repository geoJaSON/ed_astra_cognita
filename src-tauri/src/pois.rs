//! Points of interest from the Galactic Exploration Catalog (EDAstro), which also serves EDSM's archived Galactic
//! Mapping Project entries. Content is CC BY-NC-SA 3.0 (CMDR Orvidius); it's downloaded and cached, never bundled.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const SOURCE_URL: &str = "https://edastro.com/gec/json/combined";
const CACHE_FILE: &str = "gec_combined.json";
const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const SUMMARY_CHARS: usize = 320;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPoi {
    id: serde_json::Value,
    // Any of these can be null in the feed.
    source: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    name: Option<String>,
    gal_map_search: Option<String>,
    coordinates: Option<[f64; 3]>,
    summary: Option<String>,
    description_mardown: Option<String>,
    poi_url: Option<String>,
    gal_map_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Poi {
    /// Unique across sources, e.g. "GEC-10".
    pub id: String,
    pub source: String,
    pub name: String,
    /// One of a small fixed set used for filtering and colour.
    pub category: &'static str,
    /// The source's own type label.
    pub kind: String,
    pub system: String,
    pub pos: [f64; 3],
    pub summary: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PoiSet {
    pub pois: Vec<Poi>,
    /// Unix seconds when the cached copy was downloaded.
    pub fetched_at: u64,
    /// Set when the latest download failed and an older copy (or nothing) is being used.
    pub error: Option<String>,
}

/// Groups GEC categories and GMP types into the handful of categories the map filters on.
fn category(source: &str, kind: &str) -> &'static str {
    match (source, kind) {
        (_, "nebula" | "planetaryNebula" | "Nebulae") => "Nebulae",
        (_, "stellarRemnant" | "blackHole" | "pulsar" | "starCluster" | "Stellar Features" | "Notable Stellar Phenomena") => {
            "Stellar"
        }
        (
            _,
            "planetFeatures" | "geyserPOI" | "surfacePOI" | "Planetary Features" | "Sights and Scenery"
            | "Green Gas Giants" | "Planetary Circumnavigation",
        ) => "Planets & scenery",
        (_, "organicPOI" | "Organic") => "Organic",
        (_, "mysteryPOI" | "Mystery and Xenology" | "Glitches") => "Mystery & xeno",
        (
            _,
            "deepSpaceOutpost" | "independentOutpost" | "settlement" | "Deep Space Outpost" | "Inhabited System",
        ) => "Stations & outposts",
        (_, "historicalLocation" | "Historical" | "Memorials" | "Tourist Beacons") => "Historical",
        (_, "jumponiumRichSystem") => "Jumponium",
        _ => "Other",
    }
}

fn unescape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Reduces markdown to a short plain-text blurb: drops images and link targets, collapses whitespace.
fn plain_summary(markdown: &str) -> String {
    let mut text = String::new();
    let mut chars = markdown.chars().peekable();
    let mut prev = ' ';
    while let Some(c) = chars.next() {
        match c {
            '!' if chars.peek() == Some(&'[') => {
                // Image: skip "[alt](target)".
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            // Link target after "[text]".
            '(' if prev == ']' => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            '[' | ']' | '*' | '#' | '_' | '`' | '>' => {}
            _ => text.push(c),
        }
        prev = c;
    }
    let collapsed = unescape_html(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    match collapsed.char_indices().nth(SUMMARY_CHARS) {
        Some((cut, _)) => format!("{}…", collapsed[..cut].trim_end()),
        None => collapsed,
    }
}

/** Name reduced for duplicate detection: "The Traikeou Goliaths" and "Traikeou Goliaths" match. */
fn dedupe_key(p: &Poi) -> (i64, i64, i64, String) {
    let lower = p.name.to_lowercase();
    let name: String = lower.strip_prefix("the ").unwrap_or(&lower).chars().filter(|c| c.is_alphanumeric()).collect();
    let [x, y, z] = p.pos.map(|v| v.round() as i64);
    (x, y, z, name)
}

fn parse(json: &str) -> Result<Vec<Poi>, String> {
    let raw: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|e| format!("unexpected POI data: {e}"))?;
    // One malformed entry shouldn't cost the rest.
    let mut pois: Vec<Poi> = raw
        .into_iter()
        .filter_map(|v| RawPoi::deserialize(v).ok())
        .filter_map(|p| {
            let pos = p.coordinates?;
            let source = p.source.filter(|s| !s.is_empty()).unwrap_or_else(|| "GEC".to_string());
            let kind = p.kind.unwrap_or_default();
            let id = match &p.id {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let summary = p.summary.filter(|s| !s.trim().is_empty()).or(p.description_mardown).unwrap_or_default();
            Some(Poi {
                id: format!("{source}-{id}"),
                category: category(&source, &kind),
                source,
                name: unescape_html(p.name.unwrap_or_default().trim()),
                kind,
                system: unescape_html(p.gal_map_search.unwrap_or_default().trim()),
                pos,
                summary: plain_summary(&summary),
                url: p.poi_url.or(p.gal_map_url),
            })
        })
        .collect();
    // Many GMP entries were carried over into GEC; keep the GEC copy (it's the maintained one).
    pois.sort_by_key(|p| p.source != "GEC");
    let mut seen = HashSet::new();
    pois.retain(|p| seen.insert(dedupe_key(p)));
    Ok(pois)
}

fn download() -> Result<String, String> {
    ureq::get(SOURCE_URL)
        .header("User-Agent", concat!("AstraCognita/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| format!("download failed: {e}"))?
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_string()
        .map_err(|e| format!("download failed: {e}"))
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[derive(Clone)]
pub struct PoiCache {
    path: PathBuf,
}

impl PoiCache {
    pub fn new(cache_dir: &Path) -> Self {
        Self { path: cache_dir.join(CACHE_FILE) }
    }

    fn read_cached(&self) -> Option<(Vec<Poi>, SystemTime)> {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok()?;
        let pois = parse(&fs::read_to_string(&self.path).ok()?).ok()?;
        Some((pois, modified))
    }

    /// Returns the cached list, downloading a fresh one first if it's missing, older than a week, or `force` is set.
    /// Blocks on the network, so call it off the UI thread.
    pub fn load(&self, force: bool) -> PoiSet {
        let cached = self.read_cached();
        let stale = cached.as_ref().is_none_or(|(_, t)| t.elapsed().unwrap_or(MAX_AGE) >= MAX_AGE);
        if !force && !stale {
            let (pois, t) = cached.unwrap();
            return PoiSet { pois, fetched_at: unix_secs(t), error: None };
        }

        let fresh = download().and_then(|json| {
            let pois = parse(&json)?;
            if let Some(dir) = self.path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            fs::write(&self.path, &json).map_err(|e| format!("couldn't cache POIs: {e}"))?;
            Ok(pois)
        });
        match (fresh, cached) {
            (Ok(pois), _) => PoiSet { pois, fetched_at: unix_secs(SystemTime::now()), error: None },
            (Err(error), Some((pois, t))) => PoiSet { pois, fetched_at: unix_secs(t), error: Some(error) },
            (Err(error), None) => PoiSet { pois: Vec::new(), fetched_at: 0, error: Some(error) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_duplicates_keeping_gec() {
        let json = r#"[
            {"id":1,"type":"stellarRemnant","name":"The Traikeou Goliaths","coordinates":[10.2,0,5],"source":"GMP"},
            {"id":2,"type":"Stellar Features","name":"Traikeou Goliaths","coordinates":[10.1,0,5],"source":"GEC"},
            {"id":3,"type":"Stellar Features","name":"Another sight","coordinates":[10.1,0,5],"source":"GEC"}
        ]"#;
        let ids: Vec<String> = parse(json).unwrap().into_iter().map(|p| p.id).collect();
        assert_eq!(ids, ["GEC-2", "GEC-3"]);
    }

    #[test]
    fn parses_both_sources_and_cleans_text() {
        let json = r#"[
            {"id":10,"type":"Sights and Scenery","name":"The Ammonia Lyceum","galMapSearch":"Athaip WR-H d11-7577",
             "coordinates":[334.969,-55.4375,23014.3],"summary":"Body 3 b a is a nested moon.","source":"GEC",
             "poiUrl":"https://edastro.com/gec/view/10"},
            {"id":2432,"type":"organicPOI","name":"Eos&#39; Garden","galMapSearch":"Pha Free LC-D d12-187",
             "coordinates":[18744.03125,8.75,26928.4375],
             "descriptionMardown":"![](https://x/y.jpg \"t\")\r\n\r\nTeeming with **life** &amp; [rings](https://z).","source":"GMP"},
            {"id":1,"type":"region","name":"No coordinates","source":"GMP"},
            {"id":7,"type":null,"name":"Nulls","galMapSearch":null,"coordinates":[1,2,3],"summary":null,"source":"GMP"},
            {"id":8,"coordinates":"not a position"}
        ]"#;
        let pois = parse(json).unwrap();
        assert_eq!(pois.len(), 3);
        assert_eq!(pois[2].category, "Other");
        assert_eq!(pois[0].category, "Planets & scenery");
        assert_eq!(pois[1].id, "GMP-2432");
        assert_eq!(pois[1].name, "Eos' Garden");
        assert_eq!(pois[1].category, "Organic");
        assert_eq!(pois[1].summary, "Teeming with life & rings.");
    }
}
