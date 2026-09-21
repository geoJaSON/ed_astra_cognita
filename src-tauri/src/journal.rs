//! Locating and tailing the game's journal files and `Status.json`.

use serde::Deserialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const GAME_SUBDIR: &str = r"Frontier Developments\Elite Dangerous";

/// `ED_JOURNAL_DIR` overrides the default `Saved Games\Frontier Developments\Elite Dangerous`.
pub fn journal_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ED_JOURNAL_DIR") {
        return Some(PathBuf::from(dir));
    }
    let dir = saved_games_dir()?.join(GAME_SUBDIR);
    dir.is_dir().then_some(dir)
}

#[cfg(windows)]
fn saved_games_dir() -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_SavedGames, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_SavedGames, KF_FLAG_DEFAULT, None).ok()?;
        let result = path.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(path.0 as *const _));
        result
    }
}

#[cfg(not(windows))]
fn saved_games_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Saved Games"))
}

/// Journal files in chronological order. The `Journal.YYYY-MM-DDTHHMMSS.NN.log`
/// naming (used since 2021) sorts correctly as plain text.
fn journal_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.starts_with("Journal.") && n.ends_with(".log") && n.as_bytes().get(12) == Some(&b'-')
            })
        })
        .collect();
    files.sort();
    files
}

pub struct JournalTailer {
    dir: PathBuf,
    current: Option<PathBuf>,
    offset: u64,
    partial: Vec<u8>,
}

impl JournalTailer {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, current: None, offset: 0, partial: Vec::new() }
    }

    /// Reads journals from the one containing your most recent jump onward, so everything scanned in the current
    /// system survives any number of relogs, then leaves the tailer at the end of the newest journal.
    pub fn backfill_current_system(&mut self) -> Vec<Value> {
        let all = journal_files(&self.dir);
        let has_jump = |path: &PathBuf| {
            fs::read_to_string(path)
                .is_ok_and(|text| text.contains(r#""event":"FSDJump""#) || text.contains(r#""event":"CarrierJump""#))
        };
        let from_end = all.iter().rev().position(has_jump).map_or(all.len(), |i| i + 1);
        self.backfill(from_end)
    }

    /// Reads the last `files` journals in full, then leaves the tailer at the end of the newest one.
    pub fn backfill(&mut self, files: usize) -> Vec<Value> {
        let all = journal_files(&self.dir);
        let start = all.len().saturating_sub(files);
        let mut events = Vec::new();
        for path in &all[start..] {
            self.switch_to(path.clone());
            events.extend(self.read_new());
        }
        events
    }

    /// Returns any events written since the last call, following the game onto a new journal file.
    pub fn poll(&mut self) -> Vec<Value> {
        let mut events = self.read_new();
        if let Some(newest) = journal_files(&self.dir).pop() {
            if self.current.as_ref() != Some(&newest) {
                self.switch_to(newest);
                events.extend(self.read_new());
            }
        }
        events
    }

    fn switch_to(&mut self, path: PathBuf) {
        self.current = Some(path);
        self.offset = 0;
        self.partial.clear();
    }

    fn read_new(&mut self) -> Vec<Value> {
        let Some(path) = &self.current else { return Vec::new() };
        let Ok(mut file) = File::open(path) else { return Vec::new() };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // File was truncated or replaced; start over.
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset += buf.len() as u64;
        self.partial.extend_from_slice(&buf);

        // Only consume complete lines; the game may be mid-write on the last one.
        let Some(last_newline) = self.partial.iter().rposition(|&b| b == b'\n') else {
            return Vec::new();
        };
        let complete: Vec<u8> = self.partial.drain(..=last_newline).collect();
        String::from_utf8_lossy(&complete)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Status {
    #[serde(default)]
    pub flags: u64,
    #[serde(default)]
    pub gui_focus: u32,
    pub body_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

/// Re-reads `Status.json` whenever the game rewrites it.
pub struct StatusReader {
    path: PathBuf,
    last_modified: Option<SystemTime>,
}

impl StatusReader {
    pub fn new(dir: &Path) -> Self {
        Self { path: dir.join("Status.json"), last_modified: None }
    }

    pub fn poll(&mut self) -> Option<Status> {
        let modified = fs::metadata(&self.path).and_then(|m| m.modified()).ok()?;
        if self.last_modified == Some(modified) {
            return None;
        }
        // The game truncates then rewrites the file; a half-written read fails to parse and is retried next tick.
        let status = serde_json::from_str(&fs::read_to_string(&self.path).ok()?).ok()?;
        self.last_modified = Some(modified);
        Some(status)
    }
}
