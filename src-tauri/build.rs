use std::path::Path;

fn main() {
    // The builder script passes the owner's first name in; it becomes the
    // default masthead ("<Name>'s Daily") until they change it in the app.
    println!("cargo:rerun-if-env-changed=DAILY_OWNER_NAME");

    // The builder also draws a personal icon (the owner's initials) into
    // icons-personal/. When it's there, it goes into the .exe instead of the
    // stock "MD" one. That folder is never committed.
    let personal = Path::new("icons-personal/icon.ico");
    println!("cargo:rerun-if-changed=icons-personal/icon.ico");
    let mut windows = tauri_build::WindowsAttributes::new();
    if personal.is_file() {
        windows = windows.window_icon_path(personal);
    }
    // The same choice for the icon windows show in the title bar and taskbar:
    // lib.rs embeds whichever PNG lands in OUT_DIR.
    let png = ["icons-personal/128x128@2x.png", "icons/128x128@2x.png"]
        .into_iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .expect("icons/128x128@2x.png is missing");
    println!("cargo:rerun-if-changed=icons-personal/128x128@2x.png");
    println!("cargo:rerun-if-changed=icons/128x128@2x.png");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    std::fs::copy(png, Path::new(&out).join("window-icon.png")).expect("copy window icon");

    let attributes = tauri_build::Attributes::new().windows_attributes(windows);
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
