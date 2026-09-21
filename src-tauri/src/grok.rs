//! The X wire: xAI's Grok Build CLI in headless mode.
//!
//! X walls its data off from everyone except Grok, and the Grok CLI is
//! included with SuperGrok (sign in once with `grok login`, no API key).
//! So we run `grok -p ... --output-format json` the same way we run Claude,
//! ask it for posts, and hand the result to the editor as wire copy.
//!
//! The flag set below mirrors what is known to work on grok-cli 0.2.x:
//! a tool denylist (allowlists via --tools are broken there), a read-only
//! sandbox, no subagents, and auto-approval so a read-only turn can't block.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;

use crate::model::{Interest, WireItem};
use crate::paths;

/// Everything that can write, run a shell, fan out, or generate media.
const DENYLIST: &[&str] = &[
    "run_terminal_cmd",
    "run_terminal_command",
    "search_replace",
    "write",
    "ask_user_question",
    "Agent",
    "spawn_subagent",
    "task",
    "use_tool",
    "search_tool",
    "image_gen",
    "image_edit",
    "image_to_video",
    "reference_to_video",
    "monitor",
    "scheduler_create",
    "scheduler_delete",
    "scheduler_list",
];

pub fn build_prompt(interests: &[Interest], today: &str) -> String {
    let mut beats = String::new();
    let mut accounts = Vec::new();
    for i in interests.iter().filter(|i| i.enabled) {
        if i.kind == "x_account" {
            let h = i.value.trim().trim_start_matches('@');
            if !h.is_empty() {
                accounts.push(format!("@{h}"));
            }
        }
        beats.push_str(&format!("- {}", i.name));
        if !i.notes.trim().is_empty() {
            beats.push_str(&format!(": {}", i.notes.trim()));
        }
        beats.push('\n');
    }
    let accounts_line = if accounts.is_empty() {
        String::new()
    } else {
        format!("\nCheck these accounts first: {}\n", accounts.join(", "))
    };

    format!(
        r#"Today is {today}. You are the X (Twitter) correspondent for a one-reader personal newspaper.

Search X for posts from roughly the last 48 hours that this reader would want to see. Their interests, in priority order:
{beats}{accounts_line}
What makes a good pick: a post from the actual person or an official account, real news breaking on X, a genuinely funny or sharp post, a demo or clip worth watching. Skip engagement bait, reply-guy threads, crypto spam and rage politics.

Return 10 to 15 posts. Rules:
- Only include posts whose URL you actually saw in your search results. Never build or guess a URL.
- URLs must look like https://x.com/<handle>/status/<id>.
- "text" is the post's own words, verbatim, trimmed to 280 characters.
- Output ONLY a JSON object, no commentary, no code fence:

{{"posts":[{{"author":"Display Name","handle":"@handle","text":"...","url":"https://x.com/handle/status/123","posted_at":"2026-01-31T18:00:00Z or a plain description like '3 hours ago'","beat":"which interest this matches","why":"one short line on why the reader would care"}}]}}"#
    )
}

pub struct GrokOutcome {
    pub posts: Vec<WireItem>,
    pub note: Option<String>,
}

pub async fn x_wire(bin: &Path, cwd: &Path, prompt: &str, timeout_secs: u64) -> GrokOutcome {
    let mut output = match run_once(bin, cwd, prompt, timeout_secs, false).await {
        Ok(o) => o,
        Err(note) => return GrokOutcome { posts: vec![], note: Some(note) },
    };

    // A CLI update that renames a flag shouldn't cost us the X wire: if the
    // full flag set is rejected, retry once with the bare minimum.
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).to_lowercase();
        if err.contains("unexpected argument") || err.contains("unrecognized") || err.contains("unknown option") || err.contains("invalid value") {
            match run_once(bin, cwd, prompt, timeout_secs, true).await {
                Ok(o) => output = o,
                Err(note) => return GrokOutcome { posts: vec![], note: Some(note) },
            }
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let posts = parse_posts(&stdout);
    if !posts.is_empty() {
        return GrokOutcome { posts, note: None };
    }

    let lower = format!("{stdout}\n{stderr}").to_lowercase();
    let hint = if lower.contains("login") || lower.contains("unauthor") || lower.contains("not authenticated") || lower.contains("sign in") {
        "Grok isn't signed in. Run `grok login` in PowerShell once.".to_string()
    } else if !output.status.success() {
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        format!("Grok exited with an error: {}", crate::wire::truncate(tail.trim(), 200))
    } else {
        "Grok answered but returned no usable posts.".to_string()
    };
    GrokOutcome { posts: vec![], note: Some(format!("X wire: {hint}")) }
}

async fn run_once(bin: &Path, cwd: &Path, prompt: &str, timeout_secs: u64, minimal: bool) -> Result<std::process::Output, String> {
    let env = paths::shell_env().await;

    let mut cmd = tokio::process::Command::new(bin);
    paths::quiet_async(&mut cmd);
    cmd.arg("-p").arg(prompt).args(["--output-format", "json"]).arg("--always-approve");
    if !minimal {
        cmd.args(["--max-turns", "16"])
            .args(["--disallowed-tools", &DENYLIST.join(",")])
            .arg("--no-subagents")
            .args(["--sandbox", "read-only"])
            .arg("--no-auto-update");
    }
    cmd.current_dir(cwd)
        .env("PATH", &env.path)
        // Don't import MCP servers from other tools' configs into this run.
        .env("GROK_CLAUDE_MCPS_ENABLED", "0")
        .env("GROK_CURSOR_MCPS_ENABLED", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    match tokio::time::timeout(Duration::from_secs(timeout_secs), cmd.output()).await {
        Err(_) => Err(format!("X wire: Grok took longer than {timeout_secs}s, skipped it this edition.")),
        Ok(Err(e)) => Err(format!("X wire: couldn't start Grok ({e}).")),
        Ok(Ok(o)) => Ok(o),
    }
}

/// stdout is one JSON object: {"text": "...", "stopReason": ..., ...}.
/// `text` is the model's reply, which should itself be our JSON.
pub fn parse_posts(stdout: &str) -> Vec<WireItem> {
    let Some(envelope) = crate::editor::extract_json(stdout) else {
        return vec![];
    };
    // Either the envelope wraps the reply in "text", or (plain output) the
    // reply is the object itself.
    let reply: Value = match envelope.get("text").and_then(|t| t.as_str()) {
        Some(text) => match crate::editor::extract_json(text) {
            Some(v) => v,
            None => return vec![],
        },
        None => envelope,
    };
    let Some(arr) = reply.get("posts").and_then(|p| p.as_array()) else {
        return vec![];
    };

    arr.iter()
        .filter_map(|p| {
            let url = p.get("url")?.as_str()?.trim().to_string();
            if !is_post_url(&url) {
                return None;
            }
            let text = p.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
            if text.is_empty() {
                return None;
            }
            let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
            let handle = s("handle").map(|h| format!("@{}", h.trim_start_matches('@')));
            let author = match (s("author"), handle.clone()) {
                (Some(a), Some(h)) => Some(format!("{a} ({h})")),
                (Some(a), None) => Some(a),
                (None, h) => h,
            };
            Some(WireItem {
                kind: "post".into(),
                title: crate::wire::truncate(&text, 300),
                url: url.replace("://twitter.com/", "://x.com/"),
                source: "X".into(),
                author,
                published: s("posted_at"),
                summary: s("why"),
                image: None,
                beat: s("beat").unwrap_or_default(),
            })
        })
        .collect()
}

pub fn is_post_url(u: &str) -> bool {
    let Ok(parsed) = url::Url::parse(u) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").trim_start_matches("www.").trim_start_matches("mobile.");
    let ok_host = host == "x.com" || host == "twitter.com";
    let mut segs = parsed.path_segments().map(|s| s.collect::<Vec<_>>()).unwrap_or_default();
    segs.retain(|s| !s.is_empty());
    ok_host
        && parsed.scheme() == "https"
        && segs.len() >= 3
        && segs[1] == "status"
        && segs[2].chars().all(|c| c.is_ascii_digit())
        && !segs[2].is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_envelope_with_fenced_reply() {
        let stdout = r#"some log line
{"text":"```json\n{\"posts\":[{\"author\":\"Elon Musk\",\"handle\":\"elonmusk\",\"text\":\"Starship flight 12 is go\",\"url\":\"https://x.com/elonmusk/status/1234567890\",\"posted_at\":\"2h ago\",\"beat\":\"Elon Musk\",\"why\":\"Launch day\"},{\"author\":\"Bad\",\"text\":\"x\",\"url\":\"https://x.com/home\"}]}\n```","stopReason":"end_turn","sessionId":"s","requestId":"r"}"#;
        let posts = parse_posts(stdout);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].author.as_deref(), Some("Elon Musk (@elonmusk)"));
        assert_eq!(posts[0].url, "https://x.com/elonmusk/status/1234567890");
        assert_eq!(posts[0].kind, "post");
    }

    #[test]
    fn post_url_validation() {
        assert!(is_post_url("https://x.com/a/status/123"));
        assert!(is_post_url("https://twitter.com/a/status/123?s=20"));
        assert!(!is_post_url("https://x.com/a"));
        assert!(!is_post_url("https://x.com/a/status/abc"));
        assert!(!is_post_url("https://evil.com/a/status/123"));
        assert!(!is_post_url("http://x.com/a/status/123"));
    }

    #[test]
    fn garbage_is_empty_not_a_panic() {
        assert!(parse_posts("").is_empty());
        assert!(parse_posts("error: not logged in").is_empty());
        assert!(parse_posts(r#"{"text":"sorry, no posts"}"#).is_empty());
    }
}
