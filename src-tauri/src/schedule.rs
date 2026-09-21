//! Morning delivery: the system scheduler runs this same app with
//! `--refresh-only` once a day, so the edition is already printed and cached
//! when the window opens. The app installs and removes the job itself (the
//! switch in the masthead), so there is nothing to paste into a terminal.
//!
//! Windows: a Task Scheduler task, "My Daily Newspaper - Morning Delivery",
//! created with `schtasks`. "Run as soon as possible after a scheduled start
//! is missed" is on, so a PC that was asleep or off at 6 AM delivers when it
//! wakes. The task is the single source of truth for whether delivery is on
//! and when: status() asks Task Scheduler for it.
//!
//! macOS: a LaunchAgent, ~/Library/LaunchAgents/com.mydailynewspaper.refresh.plist
//! (launchd also runs a missed job on wake). The plist is the source of truth.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use regex::Regex;
use serde::Serialize;
use tauri::AppHandle;

use crate::store;

pub const LABEL: &str = "com.mydailynewspaper.refresh";
/// The task's name in Task Scheduler (Windows).
pub const TASK_NAME: &str = "My Daily Newspaper - Morning Delivery";
pub const REFRESH_FLAG: &str = "--refresh-only";

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleInfo {
    /// False where there is no scheduler we know how to drive (Linux).
    pub supported: bool,
    pub enabled: bool,
    pub hour: u32,
    pub minute: u32,
}

fn supported() -> bool {
    cfg!(any(target_os = "macos", windows))
}

fn agent_path(label: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{label}.plist")))
}

fn plist_path() -> Option<PathBuf> {
    agent_path(LABEL)
}

pub fn status() -> ScheduleInfo {
    let mut info = ScheduleInfo { supported: supported(), enabled: false, hour: 6, minute: 0 };
    if !info.supported {
        return info;
    }
    let time = if cfg!(windows) { task_time() } else { plist_time() };
    if let Some((h, m)) = time {
        info.enabled = true;
        info.hour = h;
        info.minute = m;
    }
    info
}

/// macOS: the time in the installed LaunchAgent, if there is one.
fn plist_time() -> Option<(u32, u32)> {
    let text = plist_path().and_then(|p| std::fs::read_to_string(p).ok())?;
    parse_time(&text)
}

/// Windows: ask Task Scheduler for the task and read its start time.
fn task_time() -> Option<(u32, u32)> {
    let mut cmd = Command::new("schtasks.exe");
    cmd.args(["/Query", "/TN", TASK_NAME, "/XML"]);
    let out = crate::paths::quiet(&mut cmd).output().ok()?;
    if !out.status.success() {
        return None;
    }
    parse_task_time(&decode_console(&out.stdout))
}

/// schtasks prints the task XML as UTF-16 or in the console code page,
/// depending on the Windows build. Everything we read from it is ASCII.
pub fn decode_console(bytes: &[u8]) -> String {
    let looks_utf16 = bytes.len() >= 4 && (bytes.starts_with(&[0xFF, 0xFE]) || (bytes[1] == 0 && bytes[3] == 0));
    if looks_utf16 {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).to_string()
    }
}

pub fn set(app: &AppHandle, enabled: bool, hour: u32, minute: u32) -> Result<ScheduleInfo, String> {
    if !supported() {
        return Err("Morning delivery uses the system scheduler, which this build only knows how to drive on Windows and macOS.".into());
    }
    let (hour, minute) = (hour.min(23), minute.min(59));
    if cfg!(windows) {
        install_task(app, enabled, hour, minute)?;
    } else {
        install_launch_agent(app, enabled, hour, minute)?;
    }
    Ok(status())
}

fn install_task(app: &AppHandle, enabled: bool, hour: u32, minute: u32) -> Result<(), String> {
    if !enabled {
        let mut cmd = Command::new("schtasks.exe");
        cmd.args(["/Delete", "/TN", TASK_NAME, "/F"]);
        // Deleting a task that isn't there is fine.
        let _ = crate::paths::quiet(&mut cmd).output();
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| format!("Can't tell where the app is installed: {e}"))?;
    let xml = task_xml(&exe.to_string_lossy(), hour, minute);
    // schtasks wants the definition as a UTF-16 file.
    let path = store::data_dir(app)?.join("morning-delivery-task.xml");
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&path, bytes).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;

    let mut cmd = Command::new("schtasks.exe");
    cmd.args(["/Create", "/TN", TASK_NAME, "/XML"]).arg(&path).arg("/F");
    let out = crate::paths::quiet(&mut cmd).output().map_err(|e| format!("Couldn't run schtasks: {e}"))?;
    let _ = std::fs::remove_file(&path);
    if !out.status.success() {
        let err = format!("{} {}", decode_console(&out.stderr).trim(), decode_console(&out.stdout).trim());
        return Err(format!("Windows wouldn't register the schedule: {}", err.trim()));
    }
    Ok(())
}

fn install_launch_agent(app: &AppHandle, enabled: bool, hour: u32, minute: u32) -> Result<(), String> {
    let path = plist_path().ok_or("Can't find your home folder.")?;
    let uid = current_uid()?;
    let domain = format!("gui/{uid}");

    // Always unload first: bootstrap refuses to replace a loaded job.
    let _ = Command::new("launchctl").args(["bootout", &format!("{domain}/{LABEL}")]).output();

    if !enabled {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("Couldn't remove {}: {e}", path.display()))?;
        }
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| format!("Can't tell where the app is installed: {e}"))?;
    let log = store::data_dir(app)?.join("background.log");
    let text = plist(&exe.to_string_lossy(), &log.to_string_lossy(), hour, minute);

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create {}: {e}", dir.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;

    let path_str = path.to_string_lossy().to_string();
    let boot = Command::new("launchctl")
        .args(["bootstrap", &domain, &path_str])
        .output()
        .map_err(|e| format!("Couldn't run launchctl: {e}"))?;
    if !boot.status.success() {
        // Older macOS spelling.
        let legacy = Command::new("launchctl").args(["load", "-w", &path_str]).output();
        let legacy_ok = legacy.as_ref().map(|o| o.status.success()).unwrap_or(false);
        if !legacy_ok {
            let err = String::from_utf8_lossy(&boot.stderr).trim().to_string();
            let _ = std::fs::remove_file(&path);
            return Err(format!("macOS wouldn't register the schedule: {err}"));
        }
    }
    Ok(())
}

fn current_uid() -> Result<String, String> {
    let out = Command::new("id").arg("-u").output().map_err(|e| format!("Couldn't run id: {e}"))?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if uid.is_empty() || !uid.chars().all(|c| c.is_ascii_digit()) {
        return Err("Couldn't work out your user id.".into());
    }
    Ok(uid)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The Task Scheduler definition. Settings that matter:
///  - StartWhenAvailable: a run missed while the PC was asleep or off happens
///    as soon as it's back, which is the whole point of a morning paper.
///  - Runs on battery, and doesn't stop if the laptop gets unplugged.
///  - InteractiveToken / LeastPrivilege: runs as you, only while you're signed
///    in, with no admin rights and no stored password.
///  - 30 minute limit: a stuck run can't sit there all day.
pub fn task_xml(exe: &str, hour: u32, minute: u32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Researches and lays out today's edition of My Daily Newspaper so it is ready when you open the app.</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2026-01-01T{hour:02}:{minute:02}:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT30M</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{flag}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        exe = xml_escape(exe),
        flag = REFRESH_FLAG,
    )
}

/// <StartBoundary>2026-01-01T06:30:00</StartBoundary> -> (6, 30)
pub fn parse_task_time(task_xml: &str) -> Option<(u32, u32)> {
    static START: OnceLock<Regex> = OnceLock::new();
    let start = START.get_or_init(|| Regex::new(r"<StartBoundary>\s*\d{4}-\d{2}-\d{2}T(\d{2}):(\d{2})").unwrap());
    let caps = start.captures(task_xml)?;
    let h: u32 = caps[1].parse().ok()?;
    let m: u32 = caps[2].parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

pub fn plist(exe: &str, log: &str, hour: u32, minute: u32) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
    <string>{flag}</string>
  </array>
  <key>StartCalendarInterval</key>
  <dict>
    <key>Hour</key>
    <integer>{hour}</integer>
    <key>Minute</key>
    <integer>{minute}</integer>
  </dict>
  <key>RunAtLoad</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{log}</string>
  <key>StandardErrorPath</key>
  <string>{log}</string>
</dict>
</plist>
"#,
        label = LABEL,
        exe = xml_escape(exe),
        flag = REFRESH_FLAG,
        log = xml_escape(log),
    )
}

pub fn parse_time(plist_text: &str) -> Option<(u32, u32)> {
    static HOUR: OnceLock<Regex> = OnceLock::new();
    static MINUTE: OnceLock<Regex> = OnceLock::new();
    let hour = HOUR.get_or_init(|| Regex::new(r"<key>Hour</key>\s*<integer>(\d{1,2})</integer>").unwrap());
    let minute = MINUTE.get_or_init(|| Regex::new(r"<key>Minute</key>\s*<integer>(\d{1,2})</integer>").unwrap());
    let h: u32 = hour.captures(plist_text)?[1].parse().ok()?;
    let m: u32 = minute.captures(plist_text).and_then(|c| c[1].parse().ok()).unwrap_or(0);
    (h < 24 && m < 60).then_some((h, m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_round_trips_and_escapes() {
        let text = plist("/Applications/My Daily Newspaper.app/Contents/MacOS/my-daily-newspaper", "/Users/r&d/Library/x.log", 6, 30);
        assert_eq!(parse_time(&text), Some((6, 30)));
        assert!(text.contains("<string>/Applications/My Daily Newspaper.app/Contents/MacOS/my-daily-newspaper</string>"));
        assert!(text.contains("<string>--refresh-only</string>"));
        assert!(text.contains("/Users/r&amp;d/Library/x.log"));
        assert!(!text.contains("r&d"));
    }

    #[test]
    fn task_definition_round_trips_and_escapes() {
        let xml = task_xml(r"C:\Users\R&D\AppData\Local\Programs\My Daily Newspaper\My Daily Newspaper.exe", 6, 5);
        assert_eq!(parse_task_time(&xml), Some((6, 5)));
        assert!(xml.contains("<StartBoundary>2026-01-01T06:05:00</StartBoundary>"));
        assert!(xml.contains(r"<Command>C:\Users\R&amp;D\AppData\Local\Programs\My Daily Newspaper\My Daily Newspaper.exe</Command>"));
        assert!(xml.contains("<Arguments>--refresh-only</Arguments>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(!xml.contains("R&D"));
        assert_eq!(parse_task_time("<Task></Task>"), None);
    }

    #[test]
    fn reads_schtasks_output_in_either_encoding() {
        let text = "<StartBoundary>2026-01-01T07:30:00</StartBoundary>";
        assert_eq!(parse_task_time(&decode_console(text.as_bytes())), Some((7, 30)));
        let mut utf16: Vec<u8> = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            utf16.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(parse_task_time(&decode_console(&utf16)), Some((7, 30)));
    }

    #[test]
    fn rejects_nonsense_times() {
        assert_eq!(parse_time("<key>Hour</key><integer>25</integer>"), None);
        assert_eq!(parse_time("no schedule here"), None);
        assert_eq!(parse_time("<key>Hour</key>\n <integer>7</integer>"), Some((7, 0)));
    }
}
