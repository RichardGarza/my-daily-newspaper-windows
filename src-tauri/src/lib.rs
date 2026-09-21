//! My Daily Newspaper - backend.
//!
//! One press of Refresh runs this pipeline:
//!   1. wire   - YouTube + news feeds (deterministic, ~seconds)      } in
//!   2. grok   - Grok CLI searches X for posts (SuperGrok login)     } parallel
//!   3. editor - Claude CLI researches, selects, writes, lays out
//!   4. content- thumbnails and share images for the chosen cards
//!   5. saved to editions/YYYY-MM-DD.json and handed to the front page
//!
//! Progress is pushed to the UI as "daily-status" events.

mod adblock;
mod editor;
mod grok;
mod model;
mod paths;
mod print;
mod reader;
mod schedule;
mod scores;
mod store;
mod wire;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use chrono::Local;
use futures::stream::{self, StreamExt};
use tauri::{AppHandle, Emitter, Manager, State};

use model::{Diagnostics, Edition, EditionStats, InterestsFile, Profile, Status};

#[derive(Default)]
struct AppState {
    refreshing: AtomicBool,
}

/// Set when the app was started by the scheduler with `--refresh-only`.
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// The owner's initials when the builder drew them, the stock icon otherwise
/// (build.rs picks). Windows shows this in the title bar and on the taskbar.
const WINDOW_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/window-icon.png"));

pub(crate) fn window_icon() -> Option<tauri::image::Image<'static>> {
    tauri::image::Image::from_bytes(WINDOW_ICON).ok()
}

fn emit_status(app: &AppHandle, stage: &str, message: &str, detail: Option<String>) {
    if HEADLESS.load(Ordering::Relaxed) {
        log_line(app, &format!("{} [{stage}] {message} {}", Local::now().format("%H:%M:%S"), detail.as_deref().unwrap_or("")));
    }
    let _ = app.emit(
        "daily-status",
        Status {
            stage: stage.to_string(),
            message: message.to_string(),
            detail,
            at: chrono::Utc::now().timestamp_millis(),
        },
    );
}

// ----------------------------------------------------------------- commands

/// Today's edition if there is one, otherwise the most recent.
#[tauri::command]
fn load_edition(app: AppHandle) -> Option<Edition> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    store::load_edition(&app, &today).or_else(|| store::load_latest_edition(&app))
}

#[tauri::command]
fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

#[tauri::command]
fn get_interests(app: AppHandle) -> Result<InterestsFile, String> {
    store::load_interests(&app)
}

#[tauri::command]
fn save_interests(app: AppHandle, file: InterestsFile) -> Result<InterestsFile, String> {
    let mut file = file;
    // Tidy up: trim, drop nameless rows, guarantee unique ids.
    let mut seen = std::collections::HashSet::new();
    file.interests.retain(|i| !i.name.trim().is_empty());
    for (n, i) in file.interests.iter_mut().enumerate() {
        i.name = i.name.trim().to_string();
        i.value = i.value.trim().to_string();
        i.notes = i.notes.trim().to_string();
        if !matches!(i.kind.as_str(), "topic" | "person" | "youtube_channel" | "x_account") {
            i.kind = "topic".into();
        }
        if i.id.trim().is_empty() || !seen.insert(i.id.clone()) {
            i.id = format!("i{}-{}", chrono::Utc::now().timestamp_millis(), n);
            seen.insert(i.id.clone());
        }
    }
    store::save_interests(&app, &file)?;
    Ok(file)
}

#[tauri::command]
fn reset_interests(app: AppHandle) -> Result<InterestsFile, String> {
    let file = store::default_interests()?;
    store::save_interests(&app, &file)?;
    Ok(file)
}

#[tauri::command]
async fn diagnostics(app: AppHandle) -> Result<Diagnostics, String> {
    let settings = store::load_settings(&app);
    let claude = paths::find_bin("claude", &settings.claude_bin).await;
    let grok = paths::find_bin("grok", &settings.grok_bin).await;
    Ok(Diagnostics {
        claude_path: claude.map(|p| p.display().to_string()),
        grok_path: grok.map(|p| p.display().to_string()),
        data_dir: store::data_dir(&app)?.display().to_string(),
    })
}

/// Current game if one is live, otherwise the last final (plus the next one).
#[tauri::command]
async fn score_bug(app: AppHandle) -> Result<scores::ScoreBug, String> {
    let team = store::load_settings(&app).mlb_team_id;
    if team == 0 {
        return Ok(scores::ScoreBug::default());
    }
    scores::fetch(&wire::http_client(), team).await
}

#[tauri::command]
fn get_profile(app: AppHandle) -> Profile {
    store::profile(&app)
}

/// Saved from the welcome page and from Edit Interests.
#[tauri::command]
fn set_profile(app: AppHandle, owner_name: String, city: String, mlb_team_id: u32, onboarded: bool) -> Result<Profile, String> {
    let clean = |s: &str, max: usize| s.trim().chars().filter(|c| !c.is_control()).take(max).collect::<String>();
    store::update_settings(&app, |s| {
        s.owner_name = clean(&owner_name, 40);
        s.city = clean(&city, 60);
        s.mlb_team_id = mlb_team_id;
        s.onboarded = onboarded;
    })?;
    let profile = store::profile(&app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&profile.paper_name);
    }
    Ok(profile)
}

#[tauri::command]
async fn print_status(app: AppHandle) -> Result<print::PrintStatus, String> {
    Ok(print::status(&app).await)
}

#[tauri::command]
async fn set_print_daily(app: AppHandle, enabled: bool) -> Result<print::PrintStatus, String> {
    store::update_settings(&app, |s| s.print_daily = enabled)?;
    Ok(print::status(&app).await)
}

/// The Print button. "preview" opens the PDF so you can look before spending
/// paper; "printer" sends it straight to the printer.
#[tauri::command]
async fn print_edition(app: AppHandle, target: String) -> Result<String, String> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    let edition = store::load_edition(&app, &today)
        .or_else(|| store::load_latest_edition(&app))
        .ok_or("There's no edition to print yet.")?;
    let pdf = print::make_pdf(&app, &edition).await?;
    if target == "printer" {
        return print::send_to_printer(&app, &pdf, &edition.date).await;
    }
    open_with_default_app(&pdf).map_err(|e| format!("Made the PDF but couldn't open it ({e}): {}", pdf.display()))?;
    Ok(pdf.display().to_string())
}

/// Hand a file to whatever the system opens it with (the PDF viewer, here).
fn open_with_default_app(path: &std::path::Path) -> std::io::Result<()> {
    let mut cmd = if cfg!(windows) {
        // explorer.exe opens a document with its default app and needs no
        // shell quoting, unlike `cmd /c start`.
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(path);
        c
    } else {
        let mut c = std::process::Command::new(if cfg!(target_os = "macos") { "open" } else { "xdg-open" });
        c.arg(path);
        c
    };
    paths::quiet(&mut cmd).spawn().map(|_| ())
}

#[tauri::command]
fn get_schedule() -> schedule::ScheduleInfo {
    schedule::status()
}

#[tauri::command]
fn set_schedule(app: AppHandle, enabled: bool, hour: u32, minute: u32) -> Result<schedule::ScheduleInfo, String> {
    schedule::set(&app, enabled, hour, minute)
}

#[tauri::command]
async fn refresh_edition(app: AppHandle, state: State<'_, AppState>) -> Result<Edition, String> {
    if state.refreshing.swap(true, Ordering::SeqCst) {
        return Err("An edition is already being built.".into());
    }
    let result = match store::RefreshLock::acquire(&app) {
        Some(_lock) => build_edition(&app).await,
        None => Err("An edition is already being built in the background. It will appear here when it's done.".into()),
    };
    state.refreshing.store(false, Ordering::SeqCst);

    match &result {
        Ok(e) => emit_status(
            &app,
            "done",
            "Edition ready",
            Some(format!("{} stories · {}s", e.cards.len(), e.stats.duration_secs)),
        ),
        Err(msg) => emit_status(&app, "error", "Stopped the presses", Some(msg.clone())),
    }
    result
}

// ----------------------------------------------------------------- pipeline

async fn build_edition(app: &AppHandle) -> Result<Edition, String> {
    let started = Instant::now();
    let settings = store::load_settings(app);
    let interests_file = store::load_interests(app)?;
    let interests = interests_file.interests;
    if !interests.iter().any(|i| i.enabled) {
        return Err("No interests are switched on. Add or enable some in Edit Interests.".into());
    }

    let data_dir = store::data_dir(app)?;
    let work_dir = store::work_dir(app)?;
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let today_long = now.format("%A, %B %-d, %Y, %-I:%M %p local time (UTC%:z)").to_string();

    emit_status(app, "wire", "Warming up the presses", Some("Finding Claude and Grok".into()));
    let claude_bin = paths::find_bin("claude", &settings.claude_bin).await.ok_or_else(|| {
        "Can't find the `claude` CLI. If it works in PowerShell, run `where.exe claude` there and put that path in settings.json as claudeBin (use forward slashes or double the backslashes).".to_string()
    })?;
    let grok_bin = if settings.use_grok {
        paths::find_bin("grok", &settings.grok_bin).await
    } else {
        None
    };

    let mut notes: Vec<String> = Vec::new();
    let client = wire::http_client();

    // 1 + 2: feeds and Grok side by side.
    emit_status(app, "wire", "Pulling the wires", Some("YouTube channels and news feeds".into()));
    let wire_fut = wire::gather(&client, &interests, data_dir.clone());
    let grok_fut = async {
        match &grok_bin {
            Some(bin) => {
                emit_status(app, "grok", "Grok is searching X", Some("Posts from the last 48 hours".into()));
                let prompt = grok::build_prompt(&interests, &today_long);
                grok::x_wire(bin, &work_dir, &prompt, settings.grok_timeout_secs).await
            }
            None => grok::GrokOutcome {
                posts: vec![],
                note: Some(if settings.use_grok {
                    "X wire: Grok CLI not found. If you have SuperGrok, install it from PowerShell (irm https://x.ai/cli/install.ps1 | iex), run `grok login`, and the X Wire section fills in. Claude covered X as best it could.".to_string()
                } else {
                    "X wire: Grok is switched off in settings.json.".to_string()
                }),
            },
        }
    };
    let (wire_result, grok_result) = tokio::join!(wire_fut, grok_fut);

    emit_status(
        app,
        "wire",
        "Wire copy is in",
        Some(format!("{} feed items · {} X posts", wire_result.items.len(), grok_result.posts.len())),
    );
    notes.extend(wire_result.notes.iter().cloned());
    if let Some(n) = &grok_result.note {
        notes.push(n.clone());
    }

    // 3: the editor.
    let prompt = editor::build_prompt(
        &interests,
        &wire_result.items,
        &grok_result.posts,
        grok_result.note.as_deref(),
        &today_long,
        settings.max_cards.clamp(8, 60),
        &store::paper_name(&store::owner_name(&settings)),
    );
    let status_app = app.clone();
    let out = editor::run(&claude_bin, &work_dir, &prompt, &settings, move |stage, message, detail| {
        emit_status(&status_app, stage, message, detail)
    })
    .await?;

    // 3b: the copy desk. Stories under length leave holes in the columns;
    // send them back to the same session once.
    let mut out = out;
    let short = editor::short_cards(&out.cards).len();
    if short > 0 {
        if let Some(sid) = out.session_id.clone() {
            emit_status(app, "writing", "Copy desk", Some(format!("Filling out {short} short {}", if short == 1 { "story" } else { "stories" })));
            let fixed = editor::lengthen_short_stories(&claude_bin, &work_dir, &settings, &sid, &mut out.cards).await;
            let left = editor::short_cards(&out.cards).len();
            if left > 0 {
                notes.push(format!("{left} of {short} short stories couldn't be filled out from the research ({fixed} were)."));
            }
        }
    }

    // 4: pictures.
    emit_status(app, "content", "Loading web content", Some("Thumbnails and lead images".into()));
    let mut cards = out.cards;
    let wire_images: std::collections::HashMap<&str, &str> = wire_result
        .items
        .iter()
        .filter_map(|w| w.image.as_deref().map(|img| (w.url.as_str(), img)))
        .collect();
    for c in cards.iter_mut() {
        if c.kind == "video" {
            if let Some(id) = wire::youtube_id(&c.url) {
                c.image = Some(wire::youtube_thumb(&id));
            }
        } else if c.image.is_none() {
            if let Some(img) = wire_images.get(c.url.as_str()) {
                c.image = Some((*img).to_string());
            } else if let Some(id) = c.video_url.as_deref().and_then(wire::youtube_id) {
                // An article about a video with no picture of its own: use the video's.
                c.image = Some(wire::youtube_thumb(&id));
            }
        }
    }
    // Only the cards big enough to show a picture are worth a page fetch.
    let need: Vec<(usize, String)> = cards
        .iter()
        .enumerate()
        .filter(|(_, c)| c.kind == "article" && c.image.is_none() && c.size != "brief")
        .map(|(i, c)| (i, c.url.clone()))
        .collect();
    let found: Vec<(usize, Option<String>)> = stream::iter(need)
        .map(|(i, url)| {
            let client = client.clone();
            async move { (i, wire::og_image(&client, &url).await) }
        })
        .buffer_unordered(6)
        .collect()
        .await;
    for (i, img) in found {
        cards[i].image = img;
    }
    // Plain-http images get blocked inside the app's web view; nearly every image
    // host answers on https, and a miss just hides the picture.
    for c in cards.iter_mut() {
        if let Some(img) = &c.image {
            if let Some(rest) = img.strip_prefix("http://") {
                c.image = Some(format!("https://{rest}"));
            }
        }
    }

    // 5: file it.
    let mut dates = store::edition_dates(app);
    if !dates.contains(&date) {
        dates.push(date.clone());
    }
    let edition = Edition {
        date: date.clone(),
        generated_at: Local::now().to_rfc3339(), // when it came off the press, not when the run began
        edition_no: dates.len() as u32,
        tagline: out.tagline,
        sections: out.sections,
        cards,
        notes,
        stats: EditionStats {
            duration_secs: started.elapsed().as_secs(),
            searches: out.searches,
            pages_read: out.pages_read,
            wire_items: wire_result.items.len(),
            x_posts_from_grok: grok_result.posts.len(),
            cost_usd: out.cost_usd,
        },
    };
    store::save_edition(app, &edition)?;
    Ok(edition)
}

// ----------------------------------------------------------------- headless

/// One line of the background run's diary. macOS's scheduler captures stdout
/// into background.log by itself; a Windows app has no stdout at all, so
/// there the line is appended to the file directly.
fn log_line(app: &AppHandle, line: &str) {
    println!("{line}");
    if !cfg!(windows) {
        return;
    }
    use std::io::Write;
    if let Ok(dir) = store::data_dir(app) {
        let path = dir.join("background.log");
        // Keep the diary small: start over past half a megabyte.
        if std::fs::metadata(&path).map(|m| m.len() > 512 * 1024).unwrap_or(false) {
            let _ = std::fs::remove_file(&path);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// What the scheduler runs: print today's edition with no window, then quit.
async fn refresh_headless(app: AppHandle) -> i32 {
    let today = Local::now().format("%Y-%m-%d").to_string();
    log_line(&app, &format!("\n=== {} scheduled refresh ===", Local::now().format("%Y-%m-%d %H:%M:%S")));

    // The scheduler also fires a missed job on wake. If an edition was printed in the
    // last few hours (by hand, or by an earlier firing), leave it alone.
    if let Some(e) = store::load_edition(&app, &today) {
        let fresh = chrono::DateTime::parse_from_rfc3339(&e.generated_at)
            .map(|t| Local::now().signed_duration_since(t).num_hours() < 3)
            .unwrap_or(false);
        if fresh {
            log_line(&app, "Today's edition is less than 3 hours old. Not reprinting it to screen.");
            if let Some(msg) = print::morning_print(&app, &e).await {
                log_line(&app, &format!("Paper: {msg}"));
            }
            return 0;
        }
    }

    let Some(_lock) = store::RefreshLock::acquire(&app) else {
        log_line(&app, "Another refresh is already running. Leaving it to finish.");
        return 0;
    };

    // Just woken up? Give the network a few minutes to come back.
    // Any HTTP answer at all counts: the point is "is there a network", and
    // Claude's API host is the one this job cannot do without.
    let client = wire::http_client();
    let mut online = std::env::var_os("RD_SKIP_NETWORK_CHECK").is_some();
    for attempt in 0..18 {
        if online {
            break;
        }
        for probe in ["https://api.anthropic.com/", "https://www.microsoft.com/"] {
            if client.head(probe).send().await.is_ok() {
                online = true;
                break;
            }
        }
        if online {
            break;
        }
        if attempt == 0 {
            log_line(&app, "No network yet, waiting...");
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
    if !online {
        log_line(&app, "Still offline after 3 minutes. Giving up; the app will print an edition when it's opened.");
        return 1;
    }

    match build_edition(&app).await {
        Ok(e) => {
            log_line(&app, &format!("Edition ready: {} stories in {}s.", e.cards.len(), e.stats.duration_secs));
            if let Some(msg) = print::morning_print(&app, &e).await {
                log_line(&app, &format!("Paper: {msg}"));
            }
            0
        }
        Err(msg) => {
            log_line(&app, &format!("Failed: {msg}"));
            1
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let headless = std::env::args().any(|a| a == schedule::REFRESH_FLAG);
    HEADLESS.store(headless, Ordering::Relaxed);

    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            load_edition,
            today,
            get_interests,
            save_interests,
            reset_interests,
            diagnostics,
            get_profile,
            set_profile,
            print_status,
            set_print_daily,
            print_edition,
            get_schedule,
            set_schedule,
            refresh_edition,
            score_bug,
            reader::open_reader,
        ])
        .setup(move |app| {
            if headless {
                // No window, no taskbar button: do the job and leave.
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);

                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let code = refresh_headless(handle.clone()).await;
                    handle.exit(code);
                });
            } else {
                // tauri.conf.json sets "create": false so the scheduled run
                // never flashes a window; the normal launch makes it here.
                let config = app
                    .config()
                    .app
                    .windows
                    .first()
                    .cloned()
                    .ok_or("tauri.conf.json defines no window")?;
                let title = store::profile(app.handle()).paper_name;
                let mut window = tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?.title(title);
                if let Some(icon) = window_icon() {
                    window = window.icon(icon)?;
                }
                window.build()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // One-window app: closing the front page quits (and takes any open
            // reader windows with it) instead of lingering windowless in the background.
            if window.label() == "main" {
                if let tauri::WindowEvent::Destroyed = event {
                    window.app_handle().exit(0);
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running My Daily Newspaper");
}
