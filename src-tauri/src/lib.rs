mod bio;
mod bookmarks;
mod journal;
mod overlay;
mod pois;
mod sampling;
mod settings;
mod system;
mod travel;
mod unsold;
mod values;

use serde::{Deserialize, Serialize};
use settings::Settings;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const TICK: Duration = Duration::from_millis(250);
const TOPMOST_REFRESH: Duration = Duration::from_secs(2);
/// Gap between EDSM lookups, to go easy on its rate limit after importing many bookmarks.
const LOOKUP_PAUSE: Duration = Duration::from_secs(1);
/// Wait before asking EDSM again about systems whose request failed.
const LOOKUP_RETRY: Duration = Duration::from_secs(120);

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OverlayState {
    enabled: bool,
    force_show: bool,
    unlocked: bool,
    visible: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Place {
    name: String,
    address: u64,
    pos: [f64; 3],
    /// Edge of the sector cube the system is known to be in, when the position is only estimated.
    uncertainty_ly: Option<f64>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CarrierView {
    location: Place,
    pending_jump: Option<Place>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PoiStatus {
    count: usize,
    fetched_at: u64,
    error: Option<String>,
    loading: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    journal_dir: Option<String>,
    commander: Option<String>,
    system: Option<system::SystemView>,
    game_focused: bool,
    gui_focus: u32,
    overlay: OverlayState,
    worth_mapping_min: u64,
    worth_bio_min: u64,
    /// The route plotted in the galaxy map; empty when none.
    route: Vec<travel::Stop>,
    carrier: Option<CarrierView>,
    jump_range: Option<f64>,
    /// Number of systems in the travel history; the UI re-fetches the trail when it changes.
    history_len: usize,
    poi_status: PoiStatus,
    /// Bumped on every bookmark change; the UI re-fetches the list when it changes.
    bookmarks_rev: u64,
    bookmarks_error: Option<String>,
    /// Why EDSM lookups of bookmark positions are failing; they're retried until they get through.
    bookmarks_lookup_error: Option<String>,
    /// The body you're near or on, from Status.json.
    current_body: Option<String>,
    /// Where you are on or over a planet. Also pushed alone as a `position` event while it's the only change.
    position: Option<journal::Surface>,
    /// The species you're part way through sampling, and where each sample was taken.
    sampling: Option<sampling::Trail>,
    unsold: unsold::UnsoldView,
}

struct Core {
    tracker: system::Tracker,
    journal_dir: Option<PathBuf>,
    settings: Settings,
    settings_path: Option<PathBuf>,
    game_focused: bool,
    gui_focus: u32,
    /// Show the overlay even when the game isn't focused (for testing without the game running).
    force_show: bool,
    /// Overlay accepts the mouse so it can be dragged into place.
    unlocked: bool,
    history: Vec<travel::Stop>,
    route: Vec<travel::Stop>,
    poi_cache: Option<pois::PoiCache>,
    pois: Option<pois::PoiSet>,
    pois_loading: bool,
    current_body: Option<String>,
    position: Option<journal::Surface>,
    sampling: sampling::Sampling,
    unsold: unsold::Unsold,
    bookmarks: bookmarks::Store,
    /// EDSM lookups wait until the travel history has loaded, so systems you've visited never need one.
    lookups_ready: bool,
    lookups_running: bool,
}

/// Coordinates the journal has for a system name: the current system, your travel history (newest first) or the
/// plotted route.
fn journal_pos(
    tracker: &system::Tracker,
    history: &[travel::Stop],
    route: &[travel::Stop],
    name: &str,
) -> Option<[f64; 3]> {
    let current = tracker.current_system().filter(|(current, _)| bookmarks::same_name(current, name));
    current
        .map(|(_, pos)| pos)
        .or_else(|| history.iter().rev().chain(route).find(|s| bookmarks::same_name(&s.name, name)).map(|s| s.pos))
}

impl Core {
    /// Runs `f` on the bookmark store with a lookup of the coordinates the journal knows.
    fn with_bookmarks<R>(
        &mut self,
        f: impl FnOnce(&mut bookmarks::Store, &dyn Fn(&str) -> Option<[f64; 3]>) -> R,
    ) -> R {
        let Core { bookmarks, tracker, history, route, .. } = self;
        f(bookmarks, &|name| journal_pos(tracker, history, route, name))
    }

    fn overlay_visible(&self) -> bool {
        self.unlocked
            || (self.settings.overlay_enabled
                && (self.force_show || (self.game_focused && overlay::gui_focus_allows_overlay(self.gui_focus))))
    }

    /// Coordinates for a system: exact if you've been there or it's on your route, otherwise estimated.
    fn place(&self, system: &system::SystemRef) -> Place {
        let known = self
            .history
            .iter()
            .rev()
            .chain(self.route.iter())
            .find(|s| s.address == system.address)
            .map(|s| s.pos);
        let (pos, uncertainty_ly) = match known {
            Some(pos) => (pos, None),
            None => {
                let (pos, size) = travel::estimate_position(system.address);
                (pos, Some(size))
            }
        };
        Place { name: system.name.clone(), address: system.address, pos, uncertainty_ly }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            journal_dir: self.journal_dir.as_ref().map(|d| d.display().to_string()),
            commander: self.tracker.commander.clone(),
            system: self.tracker.view(),
            game_focused: self.game_focused,
            gui_focus: self.gui_focus,
            overlay: OverlayState {
                enabled: self.settings.overlay_enabled,
                force_show: self.force_show,
                unlocked: self.unlocked,
                visible: self.overlay_visible(),
            },
            worth_mapping_min: self.settings.worth_mapping_min,
            worth_bio_min: self.settings.worth_bio_min,
            route: self.route.clone(),
            carrier: self.tracker.carrier.as_ref().map(|c| CarrierView {
                location: self.place(&c.location),
                pending_jump: c.pending_jump.as_ref().map(|p| self.place(p)),
            }),
            jump_range: self.tracker.jump_range,
            history_len: self.history.len(),
            poi_status: PoiStatus {
                count: self.pois.as_ref().map_or(0, |p| p.pois.len()),
                fetched_at: self.pois.as_ref().map_or(0, |p| p.fetched_at),
                error: self.pois.as_ref().and_then(|p| p.error.clone()),
                loading: self.pois_loading,
            },
            bookmarks_rev: self.bookmarks.rev(),
            bookmarks_error: self.bookmarks.error(),
            bookmarks_lookup_error: self.bookmarks.lookup_error(),
            current_body: self.current_body.clone(),
            position: self.position.clone(),
            sampling: self.sampling.trail().cloned(),
            unsold: self.unsold.view(),
        }
    }

    fn save_settings(&self) {
        if let Some(path) = &self.settings_path {
            settings::save(path, &self.settings);
        }
    }
}

struct AppState(Mutex<Core>);

fn core(app: &AppHandle) -> MutexGuard<'_, Core> {
    app.state::<AppState>().inner().0.lock().unwrap()
}

fn emit_snapshot(app: &AppHandle) {
    let snapshot = core(app).snapshot();
    let _ = app.emit("snapshot", snapshot);
}

#[tauri::command]
fn get_snapshot(app: AppHandle) -> Snapshot {
    core(&app).snapshot()
}

#[tauri::command]
fn get_history(app: AppHandle) -> Vec<travel::Stop> {
    core(&app).history.clone()
}

#[tauri::command]
fn get_pois(app: AppHandle) -> Vec<pois::Poi> {
    core(&app).pois.as_ref().map(|p| p.pois.clone()).unwrap_or_default()
}

#[tauri::command]
fn refresh_pois(app: AppHandle) {
    load_pois(app, true);
}

/// Loads POIs on a background thread, downloading when the cache is stale or `force` is set.
fn load_pois(app: AppHandle, force: bool) {
    let cache = {
        let mut core = core(&app);
        if core.pois_loading {
            return;
        }
        let Some(cache) = core.poi_cache.clone() else { return };
        core.pois_loading = true;
        cache
    };
    emit_snapshot(&app);
    std::thread::spawn(move || {
        let set = cache.load(force);
        {
            let mut core = core(&app);
            core.pois_loading = false;
            // Keep the POIs already loaded if a refresh failed outright.
            match core.pois.as_mut() {
                Some(existing) if set.pois.is_empty() => existing.error = set.error,
                _ => core.pois = Some(set),
            }
        }
        emit_snapshot(&app);
    });
}

#[tauri::command]
fn list_bookmarks(app: AppHandle) -> Vec<bookmarks::Bookmark> {
    core(&app).bookmarks.list().to_vec()
}

#[tauri::command]
fn save_bookmark(app: AppHandle, input: bookmarks::BookmarkInput) -> Result<bookmarks::Bookmark, String> {
    let saved = core(&app).with_bookmarks(|store, known| store.save(input, known))?;
    start_lookups(&app);
    emit_snapshot(&app);
    Ok(saved)
}

#[tauri::command]
fn delete_bookmark(app: AppHandle, id: String) -> Result<(), String> {
    core(&app).bookmarks.delete(&id)?;
    emit_snapshot(&app);
    Ok(())
}

#[tauri::command]
fn export_bookmarks(app: AppHandle, path: String) -> Result<usize, String> {
    core(&app).bookmarks.export(Path::new(&path))
}

#[tauri::command]
fn import_bookmarks(app: AppHandle, path: String) -> Result<bookmarks::ImportResult, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("couldn't read {path}: {e}"))?;
    let result = core(&app).with_bookmarks(|store, known| store.import(&bytes, known))?;
    start_lookups(&app);
    emit_snapshot(&app);
    Ok(result)
}

/// Looks up bookmarked systems on EDSM one at a time on a background thread until none are left to try. Systems whose
/// request failed get another pass after `LOOKUP_RETRY`, for as long as they still need a position.
fn start_lookups(app: &AppHandle) {
    {
        let mut core = core(app);
        if !core.lookups_ready || core.lookups_running {
            return;
        }
        core.lookups_running = true;
    }
    let app = app.clone();
    std::thread::spawn(move || loop {
        // Checked and cleared under the lock, so a bookmark saved meanwhile is picked up here or starts a new run.
        let (next, retry) = {
            let mut core = core(&app);
            let next = core.bookmarks.next_lookup();
            let retry = next.is_none() && core.bookmarks.end_lookups();
            core.lookups_running = next.is_some() || retry;
            (next, retry)
        };
        let Some(system) = next else {
            if !retry {
                // Shows the lookup error cleared, if there was one.
                emit_snapshot(&app);
                return;
            }
            // Bookmarks saved meanwhile join the next pass.
            std::thread::sleep(LOOKUP_RETRY);
            continue;
        };
        match bookmarks::edsm_lookup(&system) {
            Ok(found) => {
                let changed = core(&app).bookmarks.apply_lookup(&system, found);
                if changed {
                    emit_snapshot(&app);
                }
            }
            Err(e) => {
                eprintln!("{e}");
                core(&app).bookmarks.lookup_unreachable(&system, e);
                emit_snapshot(&app);
            }
        }
        std::thread::sleep(LOOKUP_PAUSE);
    });
}

#[tauri::command]
fn set_overlay_enabled(app: AppHandle, enabled: bool) {
    {
        let mut core = core(&app);
        core.settings.overlay_enabled = enabled;
        core.save_settings();
    }
    emit_snapshot(&app);
}

#[tauri::command]
fn set_worth_mapping_min(app: AppHandle, min: u64) {
    {
        let mut core = core(&app);
        core.settings.worth_mapping_min = min;
        core.save_settings();
    }
    emit_snapshot(&app);
}

#[tauri::command]
fn set_worth_bio_min(app: AppHandle, min: u64) {
    {
        let mut core = core(&app);
        core.settings.worth_bio_min = min;
        core.save_settings();
    }
    emit_snapshot(&app);
}

#[tauri::command]
fn set_overlay_force_show(app: AppHandle, force: bool) {
    core(&app).force_show = force;
    emit_snapshot(&app);
}

#[tauri::command]
fn set_overlay_unlocked(app: AppHandle, unlocked: bool) -> Result<(), String> {
    let window = app.get_webview_window("overlay").ok_or("overlay window missing")?;
    window.set_ignore_cursor_events(!unlocked).map_err(|e| e.to_string())?;
    {
        let mut core = core(&app);
        core.unlocked = unlocked;
        if !unlocked {
            if let Ok(pos) = window.outer_position() {
                core.settings.overlay_position = Some((pos.x, pos.y));
                core.save_settings();
            }
        }
    }
    emit_snapshot(&app);
    Ok(())
}

fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO)
}

/// Background loop: tails the journal, watches Status.json and the foreground window.
fn watch(app: AppHandle, overlay_hwnd: Option<usize>) {
    let dir = core(&app).journal_dir.clone();
    let mut tailer = dir.clone().map(journal::JournalTailer::new);
    let mut status = dir.as_deref().map(journal::StatusReader::new);
    let mut route = dir.as_deref().map(travel::RouteReader::new);
    if let Some(history) = dir.as_deref().map(travel::history) {
        core(&app).history = history;
    }
    let mut foreground = overlay::ForegroundWatcher::default();
    let mut last_raise = Instant::now() - TOPMOST_REFRESH;

    if let Some(t) = tailer.as_mut() {
        let (older, events) = t.backfill_with_history(unsold::wants);
        let mut core = core(&app);
        for ev in &older {
            core.unsold.apply(ev);
        }
        for ev in &events {
            core.tracker.apply(ev);
            core.unsold.apply(ev);
            // Positions of past samples aren't known; the saved trail has any that were seen live.
            core.sampling.apply(ev, None);
        }
    }
    if let Some(r) = route.as_mut().and_then(|r| r.poll()) {
        core(&app).route = r;
    }
    // Bookmarks get what the journal knows before anything goes to EDSM.
    {
        let mut core = core(&app);
        core.with_bookmarks(|store, known| store.fill_positions(known));
        core.lookups_ready = true;
    }
    start_lookups(&app);
    emit_snapshot(&app);

    loop {
        let events = tailer.as_mut().map(|t| t.poll()).unwrap_or_default();
        let new_status = status.as_mut().and_then(|s| s.poll());
        let new_route = route.as_mut().and_then(|r| r.poll());
        let focused = foreground.game_is_foreground();

        let (changed, moved, visible) = {
            let mut core = core(&app);
            let mut changed = false;
            let mut moved = false;
            // Status first, so a sample is pinned to where you are when its journal line arrives.
            if let Some(s) = new_status {
                if s.gui_focus != core.gui_focus {
                    core.gui_focus = s.gui_focus;
                    changed = true;
                }
                let body = s.body_name.as_deref().map(str::trim).filter(|b| !b.is_empty()).map(str::to_string);
                if body != core.current_body {
                    core.current_body = body;
                    changed = true;
                }
                let position = s.surface();
                if position != core.position {
                    core.position = position;
                    moved = true;
                }
            }
            for ev in &events {
                changed |= core.tracker.apply(ev);
                changed |= core.unsold.apply(ev);
                let pos = core.position.as_ref().map(|p| [p.lat, p.lon]);
                changed |= core.sampling.apply(ev, pos);
                if matches!(ev["event"].as_str(), Some("FSDJump" | "CarrierJump")) {
                    if let Ok(stop) = travel::Stop::deserialize(ev) {
                        changed |= core
                            .bookmarks
                            .fill_positions(|name| bookmarks::same_name(name, &stop.name).then_some(stop.pos));
                        if core.history.last().is_none_or(|last| last.address != stop.address) {
                            core.history.push(stop);
                        }
                    }
                }
            }
            if let Some(r) = new_route {
                core.route = r;
                // A bookmark EDSM doesn't know may be on the route you just plotted.
                core.with_bookmarks(|store, known| store.fill_positions(known));
                changed = true;
            }
            if focused != core.game_focused {
                core.game_focused = focused;
                changed = true;
            }
            (changed, moved, core.overlay_visible())
        };
        if changed {
            emit_snapshot(&app);
        } else if moved {
            // Walking rewrites Status.json constantly; only the overlay needs it, so skip rebuilding the snapshot.
            let position = core(&app).position.clone();
            let _ = app.emit("position", position);
        }

        #[cfg(windows)]
        if let Some(hwnd) = overlay_hwnd.filter(|_| visible && (changed || last_raise.elapsed() >= TOPMOST_REFRESH)) {
            overlay::raise_topmost(windows::Win32::Foundation::HWND(hwnd as *mut _));
            last_raise = Instant::now();
        }
        #[cfg(not(windows))]
        let _ = (overlay_hwnd, visible, &mut last_raise);

        std::thread::sleep(TICK);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed && shortcut == &toggle_shortcut() {
                        let enabled = !core(app).settings.overlay_enabled;
                        set_overlay_enabled(app.clone(), enabled);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let settings_path = app.path().app_config_dir().ok().map(|d| d.join("settings.json"));
            let settings = settings_path.as_deref().map(settings::load).unwrap_or_default();
            let saved_position = settings.overlay_position;
            app.manage(AppState(Mutex::new(Core {
                tracker: system::Tracker::default(),
                journal_dir: journal::journal_dir(),
                settings,
                settings_path,
                game_focused: false,
                gui_focus: 0,
                force_show: std::env::var_os("ED_OVERLAY_FORCE_SHOW").is_some(),
                unlocked: false,
                history: Vec::new(),
                route: Vec::new(),
                poi_cache: app.path().app_cache_dir().ok().map(|d| pois::PoiCache::new(&d)),
                pois: None,
                pois_loading: false,
                current_body: None,
                position: None,
                sampling: sampling::Sampling::open(
                    app.path().app_data_dir().ok().map(|d| d.join(sampling::FILE_NAME)),
                ),
                unsold: unsold::Unsold::default(),
                bookmarks: bookmarks::Store::open(app.path().app_data_dir().ok().map(|d| d.join(bookmarks::FILE_NAME))),
                lookups_ready: false,
                lookups_running: false,
            })));

            let mut overlay_hwnd = None;
            if let Some(window) = app.get_webview_window("overlay") {
                window.set_ignore_cursor_events(true)?;
                if let Some((x, y)) = saved_position {
                    window.set_position(PhysicalPosition::new(x, y))?;
                }
                #[cfg(windows)]
                {
                    let hwnd = window.hwnd()?;
                    overlay::make_non_activating(hwnd);
                    // HWND isn't Send; pass the raw handle to the watcher thread.
                    overlay_hwnd = Some(hwnd.0 as usize);
                }
            }

            app.global_shortcut().register(toggle_shortcut())?;

            let handle = app.handle().clone();
            std::thread::spawn(move || watch(handle, overlay_hwnd));
            load_pois(app.handle().clone(), false);
            Ok(())
        })
        .on_window_event(|window, event| {
            // The overlay window would otherwise keep the app alive after the main window closes.
            if window.label() == "main" && matches!(event, WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            set_overlay_enabled,
            set_overlay_force_show,
            set_overlay_unlocked,
            set_worth_mapping_min,
            set_worth_bio_min,
            get_history,
            get_pois,
            refresh_pois,
            list_bookmarks,
            save_bookmark,
            delete_bookmark,
            export_bookmarks,
            import_bookmarks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Replays the real journal folder and prints the current system. Run with
    /// `cargo test dump_current_system -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dump_current_system() {
        let dir = journal::journal_dir().expect("journal folder not found");
        let mut tracker = system::Tracker::default();
        for ev in journal::JournalTailer::new(dir).backfill_current_system() {
            tracker.apply(&ev);
        }
        println!("{}", serde_json::to_string_pretty(&tracker.view()).unwrap());
    }

    /// Replays every journal and compares the unsold estimate with what each sale paid. Run with
    /// `cargo test backtest_unsold -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn backtest_unsold() {
        let dir = journal::journal_dir().expect("journal folder not found");
        let mut ledger = unsold::Unsold::default();
        for ev in journal::JournalTailer::new(dir).backfill(usize::MAX) {
            match ev["event"].as_str() {
                Some("MultiSellExplorationData" | "SellExplorationData") => {
                    let names: Vec<&str> = ev["Discovered"]
                        .as_array()
                        .or(ev["Systems"].as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|d| d["SystemName"].as_str().or(d.as_str()))
                        .collect();
                    // TotalEarnings can be net of deductions; BaseValue + Bonus is what the data was worth.
                    let paid = ev["BaseValue"].as_u64().unwrap_or(0) + ev["Bonus"].as_u64().unwrap_or(0);
                    let estimate = ledger.estimate_sale(&names);
                    println!(
                        "{} carto {} systems: paid {paid}, estimated {estimate} ({:+.1}%)",
                        ev["timestamp"].as_str().unwrap_or_default(),
                        names.len(),
                        (estimate as f64 / paid as f64 - 1.0) * 100.0
                    );
                    for name in &names {
                        println!("    {name}: {}", ledger.estimate_sale(&[name]));
                    }
                }
                Some("SellOrganicData") => {
                    let paid: u64 = ev["BioData"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|b| b["Value"].as_u64().unwrap_or(0) + b["Bonus"].as_u64().unwrap_or(0))
                        .sum();
                    let before = ledger.view();
                    println!(
                        "{} bio: paid {paid}, carrying {} species estimated {}",
                        ev["timestamp"].as_str().unwrap_or_default(),
                        before.bio_species,
                        before.bio_value
                    );
                }
                _ => {}
            }
            ledger.apply(&ev);
        }
        println!("unsold now: {:?}", ledger.view());
    }

    /// Replays every journal and checks each species you finished analysing was among the predictions for that
    /// body. Run with `cargo test backtest_bio_predictions -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn backtest_bio_predictions() {
        let dir = journal::journal_dir().expect("journal folder not found");
        let mut tracker = system::Tracker::default();
        let (mut hits, mut misses) = (0, Vec::new());
        for ev in journal::JournalTailer::new(dir).backfill(usize::MAX) {
            tracker.apply(&ev);
            if ev["event"] != "ScanOrganic" || ev["ScanType"] != "Analyse" {
                continue;
            }
            let species = ev["Species_Localised"].as_str().unwrap_or_default();
            let view = tracker.view().expect("organic scan outside a known system");
            // The game sometimes logs a delayed analysis after you've jumped, tagged with the old system.
            if ev["SystemAddress"].as_u64() != Some(view.address) {
                continue;
            }
            let body = view.bodies.iter().find(|b| Some(b.id as u64) == ev["Body"].as_u64());
            let predicted = body
                .and_then(|b| b.bio.as_ref())
                .and_then(|bio| bio.genera.iter().find(|g| g.sampled.as_ref().is_some_and(|s| s.species == species)))
                .is_some_and(|g| g.candidates.iter().any(|c| c.name == species));
            if predicted {
                hits += 1;
            } else {
                let name = body.map_or("<body not scanned>".to_string(), |b| b.name.clone());
                let genera: Vec<String> = body
                    .and_then(|b| b.bio.as_ref())
                    .map(|bio| {
                        bio.genera
                            .iter()
                            .map(|g| {
                                let names: Vec<&str> = g.candidates.iter().map(|c| c.name.as_str()).collect();
                                format!("{}{:?}", g.name, names)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                misses.push(format!(
                    "{species} on {name} (body {}, system {}, region {:?}) predicted: {}",
                    ev["Body"],
                    view.name,
                    bio::region_at(view.star_pos),
                    genera.join(" ")
                ));
            }
        }
        println!("predicted {hits} of {} analysed species", hits + misses.len());
        for m in &misses {
            println!("  missed: {m}");
        }
    }
}
