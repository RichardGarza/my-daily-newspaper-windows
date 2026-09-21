//! The reader: a floating native window that loads the article / video / post
//! directly. Because it is a real top-level page (not an iframe), sites can't
//! refuse to be framed.
//!
//! The window loads untrusted remote content, so it gets NO access to the
//! app's commands: Tauri permission-checks every call from a non-local page,
//! and capabilities/default.json grants nothing to remote origins.
//! The on-page Close / Full Screen buttons talk to Rust without IPC: they
//! navigate to a sentinel URL, which `on_navigation` intercepts and cancels.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{adblock, store};

const SENTINEL_HOST: &str = "reader.my-daily.invalid";
static COUNTER: AtomicU32 = AtomicU32::new(1);

const READER_JS: &str = include_str!("reader_inject.js");

#[tauri::command]
pub async fn open_reader(app: AppHandle, url: String, title: Option<String>) -> Result<(), String> {
    let mut parsed = url::Url::parse(url.trim()).map_err(|_| "That link isn't a valid URL.".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Only http and https links can be opened.".into());
    }
    // HTTPS-first, like a browser: macOS refuses plain-http page loads inside
    // apps anyway (App Transport Security), so an http link would just open a
    // blank window. Local dev servers are left alone.
    let loopback = matches!(parsed.host_str(), Some("localhost") | Some("127.0.0.1") | Some("[::1]"));
    if parsed.scheme() == "http" && parsed.port().is_none() && !loopback {
        let _ = parsed.set_scheme("https");
    }

    let settings = store::load_settings(&app);
    let label = format!("reader-{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| store::paper_name(&store::owner_name(&settings)));

    let nav_app = app.clone();
    let nav_label = label.clone();

    // Ad blocking. The navigation handler sees every frame's navigations but
    // isn't told which page they belong to, so keep track of the page the
    // window is on: YouTube and X are exempt (see adblock.rs).
    let block_ads = settings.block_ads;
    let page_host = Arc::new(Mutex::new(parsed.host_str().unwrap_or("").to_string()));
    let page_host_nav = page_host.clone();
    // The "Ad block on/off" button in the window flips this for this window only.
    let window_blocks = Arc::new(AtomicBool::new(block_ads));
    let window_blocks_nav = window_blocks.clone();
    let script = format!("{}{}", adblock::script_config(block_ads), READER_JS);

    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
        .title(&title)
        .inner_size(1120.0, 780.0)
        .min_inner_size(420.0, 300.0)
        .focused(true)
        .always_on_top(settings.reader_always_on_top)
        .initialization_script(&script)
        .on_page_load(move |_window, payload| {
            if let Ok(mut host) = page_host.lock() {
                *host = payload.url().host_str().unwrap_or("").to_string();
            }
        })
        .on_navigation(move |u| {
            // A web page has no business steering this window into the app's
            // own internals or the local disk.
            if matches!(u.scheme(), "tauri" | "asset" | "ipc" | "file") {
                return false;
            }
            if u.host_str() != Some(SENTINEL_HOST) {
                if window_blocks_nav.load(Ordering::Relaxed) && adblock::is_ad_url(u) {
                    let on_exempt_page = page_host_nav.lock().map(|h| adblock::is_exempt_page(&h)).unwrap_or(false);
                    if !on_exempt_page {
                        #[cfg(debug_assertions)]
                        eprintln!("[adblock] cancelled navigation to {}", u.host_str().unwrap_or(""));
                        return false;
                    }
                }
                return true;
            }
            let action = u.path().trim_matches('/').to_string();
            match action.as_str() {
                "ads-off" => window_blocks.store(false, Ordering::Relaxed),
                "ads-on" => window_blocks.store(block_ads, Ordering::Relaxed),
                _ => {}
            }
            let app = nav_app.clone();
            let label = nav_label.clone();
            // Act after this callback returns; never load the sentinel page.
            tauri::async_runtime::spawn(async move {
                let Some(win) = app.get_webview_window(&label) else { return };
                match action.as_str() {
                    "close" => {
                        let _ = win.close();
                    }
                    "fullscreen" => {
                        let on = win.is_fullscreen().unwrap_or(false);
                        // A floating window can't go full screen cleanly; drop
                        // the float while full screen, restore it after.
                        if !on {
                            let _ = win.set_always_on_top(false);
                        }
                        let _ = win.set_fullscreen(!on);
                        if on {
                            let keep = store::load_settings(&app).reader_always_on_top;
                            let _ = win.set_always_on_top(keep);
                        }
                    }
                    _ => {}
                }
            });
            false
        });

    if let Some(icon) = crate::window_icon() {
        builder = builder.icon(icon).map_err(|e| format!("Couldn't set the window icon: {e}"))?;
    }

    // Sit the reader over the front page rather than wherever the OS likes.
    let over_main = app.get_webview_window("main").and_then(|main| {
        let pos = main.outer_position().ok()?;
        let size = main.outer_size().ok()?;
        let scale = main.scale_factor().ok()?;
        let (w, h) = (1120.0_f64, 780.0_f64);
        let x = pos.x as f64 / scale + ((size.width as f64 / scale - w) / 2.0).max(20.0);
        let y = pos.y as f64 / scale + ((size.height as f64 / scale - h) / 2.0).max(20.0);
        Some((x, y))
    });
    builder = match over_main {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };

    builder.build().map_err(|e| format!("Couldn't open the reader window: {e}"))?;
    Ok(())
}
