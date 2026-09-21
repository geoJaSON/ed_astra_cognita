//! Bookmarks kept by this app (the game's own can't be read or written), saved as JSON in the app data folder.
//! Coordinates come from the journal where it has them, otherwise from an EDSM lookup.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

pub const FILE_NAME: &str = "bookmarks.json";
const FILE_VERSION: u32 = 1;
const EDSM_URL: &str = "https://www.edsm.net/api-v1/system";
const EDSM_TIMEOUT: Duration = Duration::from_secs(15);
/// Where coordinates can come from; a position with any other source in a file is dropped and resolved again.
const POS_SOURCES: [&str; 3] = ["journal", "poi", "edsm"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bookmark {
    pub id: String,
    /// As typed, trimmed.
    pub system: String,
    /// Free text, e.g. "3 b" or a full body name.
    pub body: Option<String>,
    pub categories: Vec<String>,
    pub notes: String,
    pub pos: Option<[f64; 3]>,
    /// "journal" (visited, on your route or the current system), "poi" or "edsm"; None while unresolved.
    pub pos_source: Option<String>,
    /// EDSM was asked and doesn't know the system.
    pub lookup_failed: bool,
    /// Unix seconds.
    pub created_at: u64,
    /// Unix seconds of the last edit (background position fixes don't count); the newer copy wins on import.
    pub updated_at: u64,
}

/// The edit form: no `id` creates a bookmark, an `id` updates that one.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BookmarkInput {
    pub id: Option<String>,
    pub system: String,
    pub body: Option<String>,
    pub categories: Vec<String>,
    pub notes: String,
    /// Known coordinates, when bookmarking the current system or a POI.
    pub pos: Option<[f64; 3]>,
    pub pos_source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Serialize)]
struct FileOut<'a> {
    version: u32,
    bookmarks: &'a [Bookmark],
}

#[derive(Deserialize)]
struct FileIn {
    version: u32,
    bookmarks: Vec<Bookmark>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Case-insensitive comparison, ignoring surrounding whitespace.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().chars().flat_map(char::to_lowercase).eq(b.trim().chars().flat_map(char::to_lowercase))
}

/// Same system and body; no body matches an empty one.
fn same_place(a: &Bookmark, b: &Bookmark) -> bool {
    same_name(&a.system, &b.system) && same_name(a.body.as_deref().unwrap_or(""), b.body.as_deref().unwrap_or(""))
}

fn clean_body(body: Option<String>) -> Option<String> {
    body.map(|b| b.trim().to_string()).filter(|b| !b.is_empty())
}

/// Trims categories and drops empty ones and case-insensitive repeats (the first spelling wins).
fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    categories
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty() && seen.insert(c.to_lowercase()))
        .collect()
}

fn valid_pos(pos: Option<[f64; 3]>, source: Option<&str>) -> bool {
    pos.is_some_and(|p| p.iter().all(|v| v.is_finite())) && source.is_some_and(|s| POS_SOURCES.contains(&s))
}

fn set_pos(b: &mut Bookmark, pos: [f64; 3], source: &str) {
    b.pos = Some(pos);
    b.pos_source = Some(source.to_string());
    b.lookup_failed = false;
}

/// Gives bookmarks without coordinates any that `known` (the journal) has. Returns true if any changed.
fn fill_from_journal(list: &mut [Bookmark], known: impl Fn(&str) -> Option<[f64; 3]>) -> bool {
    let mut changed = false;
    for b in list.iter_mut().filter(|b| b.pos.is_none()) {
        if let Some(pos) = known(&b.system) {
            set_pos(b, pos, "journal");
            changed = true;
        }
    }
    changed
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Time plus a per-session counter; retried in the unlikely case it's already in `list`.
fn new_id(list: &[Bookmark]) -> String {
    loop {
        let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let id = format!("{nanos:x}-{:x}", ID_COUNTER.fetch_add(1, Ordering::Relaxed));
        if !list.iter().any(|b| b.id == id) {
            return id;
        }
    }
}

/// Cleans up a bookmark read from an import file. Returns false when it has no system name.
fn tidy(b: &mut Bookmark) -> bool {
    b.system = b.system.trim().to_string();
    b.body = clean_body(b.body.take());
    b.categories = normalize_categories(std::mem::take(&mut b.categories));
    if !valid_pos(b.pos, b.pos_source.as_deref()) {
        b.pos = None;
        b.pos_source = None;
    }
    b.lookup_failed &= b.pos.is_none();
    !b.system.is_empty()
}

/// Notepad and friends may add a byte-order mark, which JSON parsers reject.
fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

fn to_json(list: &[Bookmark]) -> Result<String, String> {
    serde_json::to_string_pretty(&FileOut { version: FILE_VERSION, bookmarks: list }).map_err(|e| e.to_string())
}

fn parse_file(bytes: &[u8]) -> Result<Vec<Bookmark>, String> {
    let file: FileIn = serde_json::from_slice(strip_bom(bytes)).map_err(|e| e.to_string())?;
    if file.version > FILE_VERSION {
        return Err(format!("it's from a newer version of the app (file version {})", file.version));
    }
    let mut list = file.bookmarks;
    // Edits and deletes find bookmarks by id, so a hand-edited file mustn't have blank or repeated ones.
    for i in 0..list.len() {
        if list[i].id.is_empty() || list[..i].iter().any(|b| b.id == list[i].id) {
            list[i].id = new_id(&list);
        }
    }
    Ok(list)
}

/// Bookmarks from an export file or a bare array; entries that aren't bookmarks come back as None.
fn parse_import(bytes: &[u8]) -> Result<Vec<Option<Bookmark>>, String> {
    use serde_json::Value;
    let value: Value = serde_json::from_slice(strip_bom(bytes)).map_err(|e| format!("not a JSON file: {e}"))?;
    let entries = match value {
        Value::Array(entries) => entries,
        Value::Object(mut file) => match file.remove("bookmarks") {
            Some(Value::Array(entries)) => entries,
            _ => return Err("no bookmarks in that file".into()),
        },
        _ => return Err("no bookmarks in that file".into()),
    };
    Ok(entries.into_iter().map(|v| Bookmark::deserialize(v).ok()).collect())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// Writes a temp file beside `path`, copies the current file to `backup` (if given and there is one), then renames
/// the temp file over `path`, which replaces it on Windows too.
fn write_atomic(path: &Path, contents: &str, backup: Option<&Path>) -> std::io::Result<()> {
    let tmp = with_suffix(path, ".tmp");
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        if let Some(backup) = backup.filter(|_| path.exists()) {
            fs::copy(path, backup)?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Copies an unreadable file to `<name>.corrupt.json`, or `.corrupt.1.json` and so on if an earlier copy is there.
fn set_aside(path: &Path) -> std::io::Result<PathBuf> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let mut n = 0;
    let copy = loop {
        let name = if n == 0 { format!("{stem}.corrupt.json") } else { format!("{stem}.corrupt.{n}.json") };
        let candidate = path.with_file_name(name);
        if !candidate.exists() {
            break candidate;
        }
        n += 1;
    };
    fs::copy(path, &copy)?;
    Ok(copy)
}

#[derive(Default)]
pub struct Store {
    /// None when there's no app data folder; bookmarks then last only for the session.
    path: Option<PathBuf>,
    bookmarks: Vec<Bookmark>,
    rev: u64,
    /// Problem reading the file at startup, reported for the whole session.
    load_error: Option<String>,
    /// The latest failed background save, cleared by the next successful save.
    save_error: Option<String>,
    /// The file on disk couldn't be read or copied aside, so it mustn't be overwritten.
    read_only: bool,
    /// The file on disk is the unreadable one (already copied aside), so saving mustn't make it the backup.
    bad_file: bool,
    /// (id, lowercased system) of bookmarks already sent to EDSM in this pass of lookups.
    looked_up: HashSet<(String, String)>,
    /// Lowercased systems whose EDSM request failed in this pass; the next pass offers them again.
    unreachable: HashSet<String>,
    /// Why the latest EDSM request failed; cleared once a pass gets through without failures.
    lookup_error: Option<String>,
}

impl Store {
    /// Loads the bookmarks file. Missing means none yet; unreadable means it's copied aside (or, failing that, never
    /// overwritten), the backup is loaded instead if it's readable, and the problem is reported through `error()`.
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut store = Store { rev: 1, ..Default::default() };
        let Some(path) = path else {
            store.load_error = Some("no app data folder, so bookmarks won't be kept after you close the app".into());
            return store;
        };
        let loaded = match fs::read(&path) {
            Ok(bytes) => parse_file(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.to_string()),
        };
        match loaded {
            Ok(list) => store.bookmarks = list,
            Err(problem) => {
                // The backup is the file as it was before the last save.
                let restored = fs::read(with_suffix(&path, ".bak")).ok().and_then(|bytes| parse_file(&bytes).ok());
                let outcome = match restored {
                    Some(_) => "so the list was restored from the backup, which may be missing your last change",
                    None => "so the list starts empty",
                };
                match set_aside(&path) {
                    Ok(copy) => {
                        store.bad_file = true;
                        store.load_error = Some(format!(
                            "couldn't read your bookmarks ({problem}), {outcome}; the unreadable file is kept as {}",
                            copy.display()
                        ));
                    }
                    Err(e) => {
                        store.read_only = true;
                        store.load_error = Some(format!(
                            "couldn't read {} ({problem}) or copy it aside ({e}), {outcome}; not saving bookmarks \
                             until that's fixed",
                            path.display()
                        ));
                    }
                }
                store.bookmarks = restored.unwrap_or_default();
            }
        }
        store.path = Some(path);
        store
    }

    pub fn list(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    /// Bumped on every change, including background position fixes.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn error(&self) -> Option<String> {
        match (&self.load_error, &self.save_error) {
            (Some(load), Some(save)) => Some(format!("{load}; {save}")),
            (load, save) => load.clone().or_else(|| save.clone()),
        }
    }

    /// Writes `list` to disk, keeping the previous file as `.bak` unless it's the unreadable one.
    fn write(&self, list: &[Bookmark]) -> Result<(), String> {
        if self.read_only {
            return Err("the bookmarks file couldn't be read or copied aside, so it isn't being overwritten".into());
        }
        let Some(path) = &self.path else { return Ok(()) };
        let fail = |e: std::io::Error| format!("couldn't save bookmarks: {e}");
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(fail)?;
        }
        let backup = (!self.bad_file).then(|| with_suffix(path, ".bak"));
        write_atomic(path, &to_json(list)?, backup.as_deref()).map_err(fail)
    }

    /// Makes `list` current once it's on disk; on failure nothing changes.
    fn commit(&mut self, list: Vec<Bookmark>) -> Result<(), String> {
        self.write(&list)?;
        self.bookmarks = list;
        self.save_error = None;
        self.bad_file = false;
        self.rev += 1;
        Ok(())
    }

    /// Saves changes a background task made in place, keeping them in memory even if the write fails.
    fn persist(&mut self) {
        self.rev += 1;
        self.save_error = self.write(&self.bookmarks).err();
        self.bad_file &= self.save_error.is_some();
    }

    /// Creates or updates a bookmark. Coordinates: those supplied, else what `known` (the journal) has for the system;
    /// failing both, `next_lookup` will offer it for an EDSM lookup.
    pub fn save(&mut self, input: BookmarkInput, known: impl Fn(&str) -> Option<[f64; 3]>) -> Result<Bookmark, String> {
        let system = input.system.trim().to_string();
        if system.is_empty() {
            return Err("a system name is required".into());
        }
        let mut list = self.bookmarks.clone();
        let now = now();
        let index = match &input.id {
            Some(id) => list.iter().position(|b| &b.id == id).ok_or("that bookmark no longer exists")?,
            None => {
                let id = new_id(&list);
                list.push(Bookmark { id, created_at: now, ..Default::default() });
                list.len() - 1
            }
        };
        let b = &mut list[index];
        if !same_name(&b.system, &system) {
            // Coordinates found for the old name don't apply to the new one.
            b.pos = None;
            b.pos_source = None;
            b.lookup_failed = false;
        }
        b.system = system;
        b.body = clean_body(input.body);
        b.categories = normalize_categories(input.categories);
        b.notes = input.notes;
        b.updated_at = now;
        match (input.pos, input.pos_source) {
            (Some(pos), Some(source)) if valid_pos(Some(pos), Some(&source)) => set_pos(b, pos, &source),
            _ => {
                fill_from_journal(std::slice::from_mut(b), known);
            }
        }
        let saved = b.clone();
        self.commit(list)?;
        Ok(saved)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if !self.bookmarks.iter().any(|b| b.id == id) {
            return Err("that bookmark no longer exists".into());
        }
        let list = self.bookmarks.iter().filter(|b| b.id != id).cloned().collect();
        self.commit(list)
    }

    /// Writes all bookmarks to `path` in the same format as the bookmarks file. Returns how many.
    pub fn export(&self, path: &Path) -> Result<usize, String> {
        write_atomic(path, &to_json(&self.bookmarks)?, None)
            .map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        Ok(self.bookmarks.len())
    }

    /// Merges an export file (or a bare array of bookmarks) into the list:
    /// - same id: replaced if the imported copy was edited more recently, otherwise skipped;
    /// - same system and body as an existing bookmark: skipped;
    /// - otherwise added, keeping its id if it has one.
    ///
    /// Then any bookmark without coordinates gets them from `known` (the journal) where possible.
    pub fn import(&mut self, bytes: &[u8], known: impl Fn(&str) -> Option<[f64; 3]>) -> Result<ImportResult, String> {
        let mut result = ImportResult::default();
        let mut list = self.bookmarks.clone();
        for entry in parse_import(bytes)? {
            let Some(mut b) = entry.and_then(|mut b| tidy(&mut b).then_some(b)) else {
                result.skipped += 1;
                continue;
            };
            if let Some(existing) = list.iter_mut().find(|e| !b.id.is_empty() && e.id == b.id) {
                if b.updated_at > existing.updated_at {
                    if b.pos.is_none() && same_name(&b.system, &existing.system) {
                        b.pos = existing.pos;
                        b.pos_source = existing.pos_source.clone();
                        b.lookup_failed = existing.lookup_failed;
                    }
                    if b.created_at == 0 {
                        b.created_at = existing.created_at;
                    }
                    *existing = b;
                    result.updated += 1;
                } else {
                    result.skipped += 1;
                }
            } else if list.iter().any(|e| same_place(e, &b)) {
                result.skipped += 1;
            } else {
                if b.id.is_empty() {
                    b.id = new_id(&list);
                }
                if b.created_at == 0 {
                    b.created_at = now();
                }
                b.updated_at = b.updated_at.max(b.created_at);
                list.push(b);
                result.added += 1;
            }
        }
        let filled = fill_from_journal(&mut list, known);
        if result.added + result.updated > 0 || filled {
            self.commit(list)?;
        }
        Ok(result)
    }

    /// Gives bookmarks without coordinates any that `known` (the journal) has, saving if anything changed.
    pub fn fill_positions(&mut self, known: impl Fn(&str) -> Option<[f64; 3]>) -> bool {
        let changed = fill_from_journal(&mut self.bookmarks, known);
        if changed {
            self.persist();
        }
        changed
    }

    /// The next system to look up on EDSM: a bookmark without coordinates that EDSM hasn't already failed to find and
    /// that hasn't been tried in this pass.
    pub fn next_lookup(&mut self) -> Option<String> {
        let key = |b: &Bookmark| (b.id.clone(), b.system.to_lowercase());
        let b =
            self.bookmarks.iter().find(|b| b.pos.is_none() && !b.lookup_failed && !self.looked_up.contains(&key(b)))?;
        self.looked_up.insert(key(b));
        Some(b.system.clone())
    }

    /// Why EDSM lookups are failing, while bookmarks wait for another try.
    pub fn lookup_error(&self) -> Option<String> {
        self.lookup_error.clone()
    }

    /// Records an EDSM request for `system` that failed (network, rate limit, bad response), so the UI can say so.
    pub fn lookup_unreachable(&mut self, system: &str, error: String) {
        self.unreachable.insert(system.to_lowercase());
        self.lookup_error = Some(error);
    }

    /// Ends a pass of lookups. Returns true when requests failed, after making those systems due again so another
    /// pass should follow; a pass without failures clears the error.
    pub fn end_lookups(&mut self) -> bool {
        if self.unreachable.is_empty() {
            self.lookup_error = None;
            return false;
        }
        let unreachable = std::mem::take(&mut self.unreachable);
        self.looked_up.retain(|(_, system)| !unreachable.contains(system));
        true
    }

    /// Records EDSM's answer on every bookmark of `system` still waiting for one, saving if anything changed.
    pub fn apply_lookup(&mut self, system: &str, found: Option<[f64; 3]>) -> bool {
        let mut changed = false;
        for b in
            self.bookmarks.iter_mut().filter(|b| b.pos.is_none() && !b.lookup_failed && same_name(&b.system, system))
        {
            match found {
                Some(pos) => set_pos(b, pos, "edsm"),
                None => b.lookup_failed = true,
            }
            changed = true;
        }
        if changed {
            self.persist();
        }
        changed
    }
}

/// Reads EDSM's system response: `{"name":…,"coords":{"x":…,"y":…,"z":…}}` when it knows the system, `[]` or `{}`
/// when it doesn't.
fn parse_edsm(json: &str) -> Result<Option<[f64; 3]>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("unexpected EDSM response: {e}"))?;
    let coords = &value["coords"];
    let axis = |name: &str| coords[name].as_f64();
    Ok(match (axis("x"), axis("y"), axis("z")) {
        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
        _ => None,
    })
}

/// Asks EDSM for a system's coordinates: `Ok(None)` when EDSM doesn't know it, `Err` when the request failed.
/// Blocks on the network, so call it off the UI thread.
pub fn edsm_lookup(system: &str) -> Result<Option<[f64; 3]>, String> {
    let fail = |e: ureq::Error| format!("EDSM lookup for {system} failed: {e}");
    let json = ureq::get(EDSM_URL)
        .query("systemName", system.trim())
        .query("showCoordinates", "1")
        .header("User-Agent", concat!("AstraCognita/", env!("CARGO_PKG_VERSION")))
        .config()
        .timeout_global(Some(EDSM_TIMEOUT))
        .build()
        .call()
        .map_err(fail)?
        .body_mut()
        .read_to_string()
        .map_err(fail)?;
    parse_edsm(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("astra_cognita_test_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn input(system: &str) -> BookmarkInput {
        BookmarkInput { system: system.into(), ..Default::default() }
    }

    fn unknown(_: &str) -> Option<[f64; 3]> {
        None
    }

    #[test]
    fn saves_and_loads() {
        let dir = temp_dir("round_trip");
        let path = dir.join(FILE_NAME);
        let mut store = Store::open(Some(path.clone()));
        assert!(store.error().is_none() && store.list().is_empty());

        let sol = BookmarkInput {
            body: Some(" 3 ".into()),
            categories: vec!["Home".into()],
            notes: "start".into(),
            ..input(" Sol ")
        };
        let sol = store.save(sol, |name| same_name(name, "sol").then_some([0.0; 3])).unwrap();
        assert_eq!((sol.system.as_str(), sol.body.as_deref()), ("Sol", Some("3")));
        assert_eq!((sol.pos, sol.pos_source.as_deref()), (Some([0.0; 3]), Some("journal")));
        let beagle = store.save(input("Beagle Point"), unknown).unwrap();
        assert_eq!((beagle.pos, beagle.pos_source.as_deref()), (None, None));
        assert_ne!(sol.id, beagle.id);

        // The second save kept the first version as the backup.
        assert_eq!(parse_file(&fs::read(dir.join("bookmarks.json.bak")).unwrap()).unwrap(), std::slice::from_ref(&sol));
        assert!(!dir.join("bookmarks.json.tmp").exists());

        let reloaded = Store::open(Some(path.clone()));
        assert!(reloaded.error().is_none());
        assert_eq!(reloaded.list(), [sol.clone(), beagle.clone()]);

        store.delete(&beagle.id).unwrap();
        assert!(store.delete(&beagle.id).is_err());
        assert_eq!(Store::open(Some(path)).list(), [sol]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keeps_a_copy_of_a_corrupt_file() {
        let dir = temp_dir("corrupt");
        let path = dir.join(FILE_NAME);
        fs::write(&path, "{ not json").unwrap();
        let mut store = Store::open(Some(path.clone()));
        assert!(store.list().is_empty());
        assert!(store.error().unwrap().contains("bookmarks.corrupt.json"));
        assert_eq!(fs::read_to_string(dir.join("bookmarks.corrupt.json")).unwrap(), "{ not json");

        store.save(input("Sol"), unknown).unwrap();
        assert_eq!(Store::open(Some(path.clone())).list().len(), 1);
        // The unreadable file didn't become the backup.
        assert!(!dir.join("bookmarks.json.bak").exists());

        // A second bad file doesn't replace the first copy.
        fs::write(&path, "[]").unwrap();
        Store::open(Some(path.clone()));
        assert_eq!(fs::read_to_string(dir.join("bookmarks.corrupt.json")).unwrap(), "{ not json");
        assert_eq!(fs::read_to_string(dir.join("bookmarks.corrupt.1.json")).unwrap(), "[]");

        // Something that can't even be copied aside is never written over.
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        let mut store = Store::open(Some(path.clone()));
        assert!(store.error().is_some());
        assert!(store.save(input("Sol"), unknown).is_err());
        assert!(store.list().is_empty() && path.is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn restores_the_backup() {
        let dir = temp_dir("backup");
        let path = dir.join(FILE_NAME);
        let backup = dir.join("bookmarks.json.bak");
        let mut store = Store::open(Some(path.clone()));
        let sol = store.save(input("Sol"), unknown).unwrap();
        store.save(input("Colonia"), unknown).unwrap();
        fs::write(&path, [0u8; 64]).unwrap();

        // The backup, from before the last save, stands in for a file that's been zeroed.
        let mut store = Store::open(Some(path.clone()));
        assert_eq!(store.list(), std::slice::from_ref(&sol));
        assert!(store.error().unwrap().contains("restored from the backup"));
        assert!(dir.join("bookmarks.corrupt.json").exists());

        // Saving replaces the unreadable file without making it the backup...
        let beagle = store.save(input("Beagle Point"), unknown).unwrap();
        assert_eq!(parse_file(&fs::read(&backup).unwrap()).unwrap(), std::slice::from_ref(&sol));
        assert_eq!(Store::open(Some(path.clone())).list(), [sol.clone(), beagle.clone()]);
        // ...and once the file is good again, it's backed up as usual.
        store.save(input("Maia"), unknown).unwrap();
        assert_eq!(parse_file(&fs::read(&backup).unwrap()).unwrap(), [sol, beagle]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn import_merges() {
        let mut store = Store::open(None);
        store.bookmarks = vec![
            Bookmark {
                id: "a".into(),
                system: "Sol".into(),
                notes: "old".into(),
                updated_at: 100,
                ..Default::default()
            },
            Bookmark {
                id: "b".into(),
                system: "Colonia".into(),
                body: Some("3 B".into()),
                updated_at: 100,
                ..Default::default()
            },
        ];
        let json = r#"{"version":1,"bookmarks":[
            {"id":"a","system":"Sol","notes":"new","updatedAt":200},
            {"id":"b","system":"Colonia","notes":"stale","updatedAt":50},
            {"id":"c","system":" colonia ","body":"3 b"},
            {"id":"d","system":"Beagle Point","categories":["Far"," far "],"updatedAt":10},
            {"system":"Achenar"},
            {"id":"d","system":"Beagle Point","updatedAt":5},
            {"system":"  "},
            {"system":42}
        ]}"#;
        let achenar = [67.5, -119.46875, 24.84375];
        let result = store.import(json.as_bytes(), |name| same_name(name, "achenar").then_some(achenar)).unwrap();
        assert_eq!(result, ImportResult { added: 2, updated: 1, skipped: 5 });

        let list = store.list();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].notes, "new");
        assert_eq!(list[1].notes, "");
        assert_eq!((list[2].id.as_str(), list[2].categories.as_slice()), ("d", &["Far".to_string()][..]));
        assert!(!list[3].id.is_empty() && list[3].created_at > 0);
        assert_eq!((list[3].pos, list[3].pos_source.as_deref()), (Some(achenar), Some("journal")));

        // A bare array works too; no body matches an empty one.
        let result = store.import(br#"[{"system":"SOL","body":""},{"system":"Maia"}]"#, unknown).unwrap();
        assert_eq!(result, ImportResult { added: 1, updated: 0, skipped: 1 });
        assert!(store.import(b"{\"nope\":1}", unknown).is_err());
    }

    #[test]
    fn normalizes_categories() {
        let categories = [" Bio ", "bio", "", "Mining", "BIO", "mining ", "  "].map(String::from).to_vec();
        assert_eq!(normalize_categories(categories), ["Bio", "Mining"]);
    }

    #[test]
    fn renaming_the_system_drops_its_coordinates() {
        let mut store = Store::open(None);
        let poi = BookmarkInput { pos: Some([1.0, 2.0, 3.0]), pos_source: Some("poi".into()), ..input("Sol") };
        let b = store.save(poi, unknown).unwrap();
        assert_eq!(b.pos_source.as_deref(), Some("poi"));

        let same = BookmarkInput { id: Some(b.id.clone()), body: Some(" ".into()), ..input("SOL") };
        let b = store.save(same, unknown).unwrap();
        assert_eq!((b.pos, b.body), (Some([1.0, 2.0, 3.0]), None));

        let renamed = store.save(BookmarkInput { id: Some(b.id.clone()), ..input("Achenar") }, unknown).unwrap();
        assert_eq!((renamed.pos, renamed.pos_source), (None, None));
        assert_eq!(renamed.created_at, b.created_at);

        assert!(store.save(BookmarkInput { id: Some("missing".into()), ..input("Sol") }, unknown).is_err());
        assert!(store.save(input("  "), unknown).is_err());
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn looks_up_each_bookmark_once() {
        let mut store = Store::open(None);
        for system in ["Sol", "sol", "Nowhere", "Offline"] {
            store.save(input(system), unknown).unwrap();
        }
        assert_eq!(store.next_lookup().as_deref(), Some("Sol"));
        assert!(store.apply_lookup("Sol", Some([0.0; 3])));
        // Both Sol bookmarks got the answer.
        assert_eq!(store.next_lookup().as_deref(), Some("Nowhere"));
        assert!(store.apply_lookup("Nowhere", None));
        // A failed request isn't retried in the same pass, but the next pass offers it again.
        assert_eq!(store.next_lookup().as_deref(), Some("Offline"));
        store.lookup_unreachable("Offline", "EDSM lookup for Offline failed: timeout".into());
        assert_eq!(store.next_lookup(), None);
        assert!(store.end_lookups());
        assert!(store.lookup_error().is_some());
        assert_eq!(store.next_lookup().as_deref(), Some("Offline"));
        assert!(store.apply_lookup("Offline", Some([1.0; 3])));
        assert_eq!(store.next_lookup(), None);
        // A pass without failures clears the error.
        assert!(!store.end_lookups());
        assert_eq!(store.lookup_error(), None);
        assert_eq!(store.next_lookup(), None);

        let list = store.list();
        assert!(list[..2].iter().all(|b| b.pos_source.as_deref() == Some("edsm")));
        assert!(list[2].lookup_failed && !list[3].lookup_failed);
        assert!(store.fill_positions(|name| same_name(name, "nowhere").then_some([5.0; 3])));
        assert!(!store.list()[2].lookup_failed);
    }

    #[test]
    fn parses_edsm_responses() {
        let sol = r#"{"name":"Sol","coords":{"x":0,"y":0,"z":0},"coordsLocked":true}"#;
        assert_eq!(parse_edsm(sol), Ok(Some([0.0, 0.0, 0.0])));
        let colonia = r#"{"name":"Colonia","coords":{"x":-9530.5,"y":-910.28125,"z":19808.125}}"#;
        assert_eq!(parse_edsm(colonia), Ok(Some([-9530.5, -910.28125, 19808.125])));
        assert_eq!(parse_edsm("[]"), Ok(None));
        assert_eq!(parse_edsm("{}"), Ok(None));
        assert!(parse_edsm("<html>Too many requests</html>").is_err());
    }
}
