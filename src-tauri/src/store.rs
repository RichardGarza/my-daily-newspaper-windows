//! Everything on disk: interests.json, settings.json, editions/YYYY-MM-DD.json.
//! All of it lives in the app data folder
//! (Windows: %APPDATA%\com.mydailynewspaper.app).

use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::model::{Edition, InterestsFile, Settings};

const DEFAULT_INTERESTS: &str = include_str!("../resources/default-interests.json");

pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    fs::create_dir_all(dir.join("editions")).map_err(|e| format!("create data dir: {e}"))?;
    fs::create_dir_all(dir.join("workdir")).map_err(|e| format!("create work dir: {e}"))?;
    Ok(dir)
}

/// Empty scratch folder the CLIs run in, so they never pick up a project's
/// CLAUDE.md / AGENTS.md or touch real files.
pub fn work_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("workdir"))
}

// ---------------------------------------------------------------- interests

pub fn load_interests(app: &AppHandle) -> Result<InterestsFile, String> {
    let path = data_dir(app)?.join("interests.json");
    if !path.exists() {
        fs::write(&path, DEFAULT_INTERESTS).map_err(|e| format!("seed interests: {e}"))?;
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read interests: {e}"))?;
    match serde_json::from_str::<InterestsFile>(&text) {
        Ok(mut f) => {
            if repair_known_handles(&mut f) {
                let _ = save_interests(app, &f);
            }
            Ok(f)
        }
        Err(e) => {
            // Keep the broken file for inspection, fall back to defaults.
            let _ = fs::copy(&path, path.with_extension("json.broken"));
            eprintln!("interests.json unreadable ({e}); using defaults");
            serde_json::from_str(DEFAULT_INTERESTS).map_err(|e| format!("default interests: {e}"))
        }
    }
}

/// Early copies shipped a YouTube handle that doesn't exist. Fix it in lists
/// that were saved before the correction; anything the owner typed is left alone.
fn repair_known_handles(file: &mut InterestsFile) -> bool {
    const FIXES: &[(&str, &str)] = &[("@PowerfulJRE", "@joerogan")];
    let mut changed = false;
    for interest in &mut file.interests {
        for (bad, good) in FIXES {
            if interest.kind == "youtube_channel" && interest.value.trim().eq_ignore_ascii_case(bad) {
                interest.value = (*good).to_string();
                changed = true;
            }
        }
    }
    changed
}

pub fn save_interests(app: &AppHandle, file: &InterestsFile) -> Result<(), String> {
    let path = data_dir(app)?.join("interests.json");
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    write_atomic(&path, &text)
}

pub fn default_interests() -> Result<InterestsFile, String> {
    serde_json::from_str(DEFAULT_INTERESTS).map_err(|e| format!("default interests: {e}"))
}

// ----------------------------------------------------------------- settings

pub fn load_settings(app: &AppHandle) -> Settings {
    let Ok(dir) = data_dir(app) else {
        return Settings::default();
    };
    let path = dir.join("settings.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => {
            // Write the defaults once so the knobs are discoverable.
            if let Ok(text) = serde_json::to_string_pretty(&Settings::default()) {
                let _ = fs::write(&path, text);
            }
            Settings::default()
        }
    }
}

pub fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = data_dir(app)?.join("settings.json");
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    write_atomic(&path, &text)
}

/// Read-modify-write, so a switch in the UI never clobbers hand edits.
pub fn update_settings(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let mut settings = load_settings(app);
    change(&mut settings);
    save_settings(app, &settings)?;
    Ok(settings)
}

/// The name given to the builder script, baked in at compile time.
const BUILT_FOR: Option<&str> = option_env!("DAILY_OWNER_NAME");

pub fn owner_name(settings: &Settings) -> String {
    let own = settings.owner_name.trim();
    if !own.is_empty() {
        return own.to_string();
    }
    BUILT_FOR.map(|n| n.trim().to_string()).unwrap_or_default()
}

/// "Sam" -> "Sam's Daily"; nobody -> "My Daily".
pub fn paper_name(owner: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        "My Daily".to_string()
    } else {
        format!("{owner}\u{2019}s Daily")
    }
}

pub fn profile(app: &AppHandle) -> crate::model::Profile {
    let settings = load_settings(app);
    let owner = owner_name(&settings);
    crate::model::Profile {
        paper_name: paper_name(&owner),
        owner_name: owner,
        city: settings.city.trim().to_string(),
        mlb_team_id: settings.mlb_team_id,
        onboarded: settings.onboarded,
    }
}

// ----------------------------------------------------------------- editions

pub fn save_edition(app: &AppHandle, edition: &Edition) -> Result<(), String> {
    let path = data_dir(app)?
        .join("editions")
        .join(format!("{}.json", edition.date));
    let text = serde_json::to_string_pretty(edition).map_err(|e| e.to_string())?;
    write_atomic(&path, &text)
}

/// Dates (YYYY-MM-DD) that have an edition on disk, oldest first.
pub fn edition_dates(app: &AppHandle) -> Vec<String> {
    let Ok(dir) = data_dir(app) else {
        return vec![];
    };
    let mut dates: Vec<String> = fs::read_dir(dir.join("editions"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.strip_suffix(".json").map(|s| s.to_string())
                })
                .filter(|s| s.len() == 10)
                .collect()
        })
        .unwrap_or_default();
    dates.sort();
    dates
}

pub fn load_edition(app: &AppHandle, date: &str) -> Option<Edition> {
    let path = data_dir(app).ok()?.join("editions").join(format!("{date}.json"));
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn load_latest_edition(app: &AppHandle) -> Option<Edition> {
    edition_dates(app)
        .iter()
        .rev()
        .find_map(|d| load_edition(app, d))
}

fn write_atomic(path: &std::path::Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))
}

// --------------------------------------------------------------------- lock

/// One edition at a time across processes: the window you have open and the
/// scheduled background run are separate copies of the app.
pub struct RefreshLock {
    path: PathBuf,
}

impl RefreshLock {
    /// None when another live refresh holds the lock. A lock older than
    /// 20 minutes is treated as left over from a crash.
    pub fn acquire(app: &AppHandle) -> Option<Self> {
        let path = data_dir(app).ok()?.join("refresh.lock");
        if let Ok(meta) = fs::metadata(&path) {
            let age = meta.modified().ok().and_then(|m| m.elapsed().ok());
            if matches!(age, Some(a) if a.as_secs() < 20 * 60) {
                return None;
            }
        }
        fs::write(&path, format!("pid {}\n", std::process::id())).ok()?;
        Some(Self { path })
    }
}

impl Drop for RefreshLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
