# Astra Cognita

Exploration companion for Elite Dangerous: a main window (Current System, Bookmarks, Nearby) and an in-game overlay showing which bodies are worth your time — only where a *first* (discovery, mapping, footfall) is still yours to take.

Everything comes from the game's local journal files. The only thing sent anywhere is the name of a bookmarked system the journal has no coordinates for, looked up on EDSM.

## Run

Needs Rust, Node and pnpm (WebView2 ships with Windows 11).

```sh
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # installer in src-tauri/target/release/bundle
```

## Overlay

- The game must run in **Borderless** mode; an overlay can't draw over exclusive fullscreen. No VR support.
- Shown only while the game is the focused window and no panel/map is open (it stays up in FSS and DSS modes).
- `Ctrl+Alt+O` toggles it from anywhere, including in game.
- **Move overlay** in the main window makes it draggable; **Lock overlay** saves the position.
- **Always show** keeps it visible without the game focused, for testing.

## Rules and data

- The overlay lists only these highlights, and hides itself when there are none.
- A body counts as a highlight when a first is still open and:
  - **MAP**: mapped value is at least the **Min. mapped value** slider in Current System (default 300k), or
  - **BIO**: the first-logged bonus is still available and the best-case payout is at least the **Min. bio value** slider (default 5M).
- Mapping values are estimates from the community formula in [src-tauri/data/body_values.json](src-tauri/data/body_values.json); not yet checked against in-game payouts.

## Exobiology

- Species are predicted from each planet's scan (atmosphere, gravity, temperature, pressure, volcanism, materials), its stars, and the galactic region, using rules ported from [EDMC-BioScan](https://github.com/Silarn/EDMC-BioScan).
- Predictions narrow as you play: before the DSS every genus the rules allow is shown (`?`); after the DSS only the genera found (`●`); while sampling, the species itself and the colony spacing.
- **First-logged bonus** (5x payout): assumed when nobody has set foot on the planet and the system is unpopulated, as BioScan does. Every sale in these journals so far paid exactly 4x extra on such planets.
- **In the overlay:** near a bio planet (the body `Status.json` says you're at), its genera are listed under it with the likeliest species, their colours and values, and the colony spacing. Once you're landed, in the SRV or on foot there, the overlay shows only that planet, whatever it's worth.
- **Sampling helper:** while a species is part-sampled, the overlay shows the distance and direction (relative to where you're facing) to each earlier sample, and whether you're past the colony spacing. `ScanOrganic` has no coordinates, so each sample is pinned to your `Status.json` position when it's logged; samples taken while the app was closed have no position. The trail is saved to `sampling.json` in the app data folder.
- Checked against these journals: every species analysed on a planet the app had scan data for was among its predictions (`cargo test backtest_bio_predictions -- --ignored --nocapture`).

### Unsold data

The overlay footer and Current System show what you're carrying unsold, which dying would lose. At startup the journals are read back to your last `Died`.

- **Bio:** each analysed species at its predicted payout (with the first-logged bonus under the same rule as above), removed species by species by `SellOrganicData`. Matched the two Vista Genomics sales in these journals to within 3%.
- **Cartographic:** every scanned body, plus DSS mapping, kept per system because Universal Cartographics buys a system at a time; `SellExplorationData`/`MultiSellExplorationData` clear the systems sold, and those bodies earn nothing if rescanned. Values come from the mapping formula above, so it's shown as `≈`: on the two sales here where every system was scanned in these journals it ran 23% and 64% high. `cargo test backtest_unsold -- --ignored --nocapture` compares each sale.

### Updating the species data

`src-tauri/data/bio/*.json` is generated from EDMC-BioScan and EDMC-ExploData, pinned to specific commits:

```sh
python tools/import_bio_data.py            # pinned commits
python tools/import_bio_data.py --bioscan master --explodata master
```

Both projects are **GPL-2.0**. The generated data and the ported rule engine in `src-tauri/src/bio.rs` are derived from them, so if this app is ever distributed it must be under GPL-2.0-compatible terms.

## Nearby

A top-down galaxy map (same orientation as the in-game galaxy map: x right, towards Sagittarius A* up) with a POI list beside it.

- **Your data, from the journal folder:** current position, travel trail (every `FSDJump`/`CarrierJump`), the route plotted in the galaxy map (`NavRoute.json`), your fleet carrier (`CarrierLocation`, `CarrierJumpRequest`) and jump range (`Loadout`).
  - A carrier in a system you haven't visited is placed using its system address (id64), which pins it to within its sector cube; the tooltip gives the uncertainty.
- **POIs:** the Galactic Exploration Catalog's combined feed (GEC plus EDSM's archived Galactic Mapping Project), downloaded from `edastro.com/gec/json/combined` and cached in the app cache folder for a week. **Refresh** forces a new download.
  - Content is CC BY-NC-SA 3.0 (CMDR Orvidius / EDAstro), credited in the panel. It's never bundled into this repo.
  - Entries in the same spot with the same name are merged, keeping the GEC copy.
- **Background:** the codex region map (from klightspeed's region map via EDMC-ExploData), with borders and names only when zoomed out.
- **List:** nearest POIs with distance and rough jump count (distance ÷ jump range). Filters: category, radius, and **Near route** (distance from the plotted route).

## Bookmarks

Kept by this app only; the game's own bookmarks can't be read or written.

- **Stored in** `bookmarks.json` in the app data folder (`%APPDATA%\com.astracognita.app` on Windows), as `{"version":1,"bookmarks":[...]}`.
  - Each save writes a temp file and renames it over the old one, after copying the old one to `bookmarks.json.bak`.
  - If the file can't be read at startup, it's first copied to `bookmarks.corrupt.json` (or `.corrupt.1.json` and so on, so earlier copies survive), then the list is restored from `bookmarks.json.bak` (or starts empty if that can't be read either) and the app reports the problem. The next save replaces the unreadable file without copying it over the backup. If even the copy aside fails, the file is never overwritten and changes aren't saved that session.
- **Coordinates** (for distance sorting and the Nearby map), first match wins:
  1. supplied when bookmarking the current system or a POI;
  2. the journal: the current system, your travel history or the plotted route (exact name, any case);
  3. EDSM (`edsm.net/api-v1/system`), looked up in the background one system at a time, a second apart.
  - A system EDSM doesn't know is flagged and not asked about again. A failed request (offline, EDSM down or rate-limiting) shows "EDSM unreachable" on the bookmark and is retried every two minutes.
  - A bookmark still without coordinates gets them when you jump into that system or plot a route through it.
- **Export / Import:** export writes the same format as the bookmarks file. Import also takes a bare array of bookmarks and merges:
  - same id: the copy with the newer `updatedAt` wins;
  - same system and body (any case) as an existing bookmark: skipped;
  - otherwise added.

## Environment variables

| Variable | Effect |
|---|---|
| `ED_JOURNAL_DIR` | Use this journal folder instead of `Saved Games\Frontier Developments\Elite Dangerous` |
| `ED_OVERLAY_FORCE_SHOW` | Start with **Always show** on |

## Code layout

- `src-tauri/src/journal.rs` — finds and tails the journal and `Status.json`
- `src-tauri/src/system.rs` — folds journal events into current-system state
- `src-tauri/src/values.rs` — cartography payout estimates
- `src-tauri/src/bio.rs` — exobiology species prediction (port of EDMC-BioScan's rules)
- `src-tauri/src/pois.rs` — POI download, cache and cleanup
- `src-tauri/src/travel.rs` — travel history, plotted route, id64 position estimates
- `src-tauri/src/bookmarks.rs` — bookmark storage (atomic saves, backups), import/export, EDSM coordinate lookups
- `src-tauri/src/overlay.rs` — Win32: focus detection, click-through, keep-on-top
- `src-tauri/src/lib.rs` — app wiring, commands, background watcher
- `src/overlay/`, `src/main/` — the two windows (same bundle; the window label picks the view)
- `src/highlights.ts` — "worth it" and first-credit rules shared by both windows
- `src/main/nearby/` — Nearby tab: Leaflet map (`CRS.Simple`, 1 unit = 1 ly), region background, POI list

## Tests

```sh
cd src-tauri
cargo test
cargo test dump_current_system -- --ignored --nocapture       # replay your real journals
cargo test backtest_bio_predictions -- --ignored --nocapture  # check predictions against species you've analysed
```
