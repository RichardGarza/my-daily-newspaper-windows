//! The editor-in-chief: the Claude CLI in headless mode.
//!
//! We run `claude -p --output-format stream-json`, feed it the interests and
//! the wire copy on stdin, and read its event stream line by line. Tool calls
//! in the stream become the status messages ("Claude's researching: ..."),
//! and the final `result` event carries the edition as JSON.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::model::{Card, Interest, Settings, WireItem};
use crate::paths;
use crate::wire;

pub struct EditorOutput {
    /// The CLI session, so short stories can be sent back to the same
    /// conversation (research still in context) instead of starting over.
    pub session_id: Option<String>,
    pub tagline: Option<String>,
    pub sections: Vec<String>,
    pub cards: Vec<Card>,
    pub searches: u32,
    pub pages_read: u32,
    pub cost_usd: Option<f64>,
}

// ------------------------------------------------------------------ lengths
//
// Story lengths in characters (min, max). The page is set in columns; a story
// under its minimum leaves a visible hole, so short ones are sent back once.

pub const LEAD_CHARS: (usize, usize) = (1500, 2300);
pub const FEATURE_CHARS: (usize, usize) = (900, 1350);
pub const BRIEF_CHARS: (usize, usize) = (520, 800);
pub const POST_CHARS: (usize, usize) = (240, 420);

pub fn min_chars(card: &Card) -> usize {
    if card.kind == "post" {
        return POST_CHARS.0;
    }
    match card.size.as_str() {
        "lead" => LEAD_CHARS.0,
        "feature" => FEATURE_CHARS.0,
        _ => BRIEF_CHARS.0,
    }
}

fn story_len(card: &Card) -> usize {
    card.story.as_deref().map(|s| s.chars().count()).unwrap_or(0)
}

/// Cards whose story came in meaningfully under length (10% grace).
pub fn short_cards(cards: &[Card]) -> Vec<usize> {
    cards
        .iter()
        .enumerate()
        .filter(|(_, c)| story_len(c) * 10 < min_chars(c) * 9)
        .map(|(i, _)| i)
        .collect()
}

// ------------------------------------------------------------------- prompt

pub fn build_prompt(
    interests: &[Interest],
    wire_items: &[WireItem],
    x_posts: &[WireItem],
    x_note: Option<&str>,
    today_long: &str,
    max_cards: usize,
    paper: &str,
) -> String {
    let mut beats = String::new();
    for (n, i) in interests.iter().filter(|i| i.enabled).enumerate() {
        beats.push_str(&format!("{}. {} [{}", n + 1, i.name, i.kind));
        if !i.value.trim().is_empty() && i.kind != "topic" && i.kind != "person" {
            beats.push_str(&format!(": {}", i.value.trim()));
        }
        beats.push(']');
        if !i.notes.trim().is_empty() {
            beats.push_str(&format!(" - {}", i.notes.trim()));
        }
        beats.push('\n');
    }

    let videos: Vec<&WireItem> = wire_items.iter().filter(|w| w.kind == "video").collect();
    let articles: Vec<&WireItem> = wire_items.iter().filter(|w| w.kind != "video").collect();
    let to_json = |v: &Vec<&WireItem>| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());

    let x_block = if x_posts.is_empty() {
        format!(
            "(none - {}) If your own searching surfaces a notable post with a real x.com/<handle>/status/<id> link, you may include it as a \"post\" card. Otherwise skip the X section entirely rather than padding it.",
            x_note.unwrap_or("the X wire was unavailable this run")
        )
    } else {
        serde_json::to_string(x_posts).unwrap_or_else(|_| "[]".into())
    };

    format!(
        r#"You are the editor-in-chief of "{paper}", a personal newspaper with exactly one reader. Today is {today_long}.

THE READER'S BEATS (priority order; disabled beats are already removed):
{beats}
WIRE COPY - already fetched for you. These links are real; prefer them.
VIDEOS (YouTube channel feeds):
{videos}

NEWS (news search feeds, unranked, may contain junk):
{articles}

X POSTS (from Grok's live X search):
{x_block}

YOUR JOB
1. Research - fast. The reader is watching a progress bar, and every extra round of tool calls costs them 30-60 seconds. You get exactly TWO research rounds:
   - Round 1: decide every search you need and send them all TOGETHER in one message (up to 10 WebSearch calls at once).
   - Round 2: one more message with everything else TOGETHER: the pages you want to read for the lead and features (up to 5 WebFetch calls) and any follow-up searches (up to 3).
   - Then the newsroom closes. No third round, no "one more check": write the edition with what you have. If a detail is still unknown, leave it out or say it is not yet known - a shorter accurate story beats another search. No commentary between rounds - just the tool calls, then the final JSON.
   The wire already covers the listed YouTube channels and general news for each beat, so do not re-search those. It is weak on: brand-new interviews and podcast appearances, music releases from the last few days, and videos from channels not on the list. Favor the last 48 hours; nothing older than 7 days unless it is a major release the reader would not want to miss.
2. Select the best {min_cards} to {max_cards} items across the reader's beats. Be a ruthless editor: drop junk, duplicates, listicles, SEO filler and anything stale. Never pad with junk - but this paper replaces the reader's browsing, so don't stop early either: every enabled beat with real news gets at least one story, the wire copy below is full of candidates, and an edition under {min_cards} stories only happens on a day with truly no news. Give higher-priority beats more room.
3. Write each card the way a newspaper writer files a short piece. The reader wants to get the whole story from the page itself and only click through when they want more:
   - "headline": your own headline, max 90 characters, no clickbait, no ALL CAPS. For videos you may keep the video's real title if it is already good.
   - "dek": one sentence, max 160 characters - the standfirst under the headline. It must not repeat the story's opening sentence. For "post" cards the dek is the post's own text, verbatim.
   - "story": the piece itself, in your own words, plain text, paragraphs separated by a blank line. This is a text-forward newspaper set in narrow columns, not a list of links: every story is a complete little article that leaves the reader knowing what happened. Include the specifics they would otherwise click for: who, what, the numbers, the dates, what was said, what happens next.
       * LENGTH IS A LAYOUT REQUIREMENT. The page is typeset in columns of about 45 characters per line, and a short story leaves a hole in the column. Count characters (letters, spaces and punctuation) and write toward the TOP of each range:
           lead     {lead_min}-{lead_max} characters, 4-6 paragraphs
           feature  {feature_min}-{feature_max} characters, 3-4 paragraphs
           brief    {brief_min}-{brief_max} characters, 2 paragraphs - a brief is still a full little story, never two sentences
           post     {post_min}-{post_max} characters of context
         Anything under the minimum is rejected and sent back to you. If you cannot reach the minimum truthfully from what you found, the item is too thin to run: either spend one of your fetches on it, or drop it and pick a meatier one. Never pad with filler or repeat yourself to make length - add real detail: background, the numbers, who else is involved, what came before, what happens next.
       * VOICE: a feature writer filing copy, never a catalog entry. Do not open with "This video...", "This article...", "In this episode..." or any description of the item as an item. Open with the most interesting concrete thing in it: what was said, shown, decided, scored or shipped. Shape of a good video opener: "In the latest episode of <show>, <guest> <the most striking thing they said or did>..." and then the rest of what it covers.
       * video cards: you have not watched the video, so work like a reporter - use its title and description, and for a lead or feature video spend a search or a fetch on recaps and coverage of it so you can report what was actually said or shown. Say who is in it and how long it runs if you know. Never invent quotes, moments or conclusions; if coverage is thin, write a shorter story about who the guest is and what the description says they get into.
       * post cards: the story is a short paragraph of context - who this is, what they are responding to, what happened around it, why it matters.
   - "why": optional, max 90 characters, only when the link to the reader's interests is not obvious.
   GROUNDING: write only what the wire copy, your search results or pages you fetched actually support. When all you have is a headline and a snippet, that item is not ready to run: fetch it or choose another. Do not guess. Never copy sentences from a source; quote sparingly, never more than 15 words, and only words you actually saw.
4. Lay out the page:
   - Exactly one card has "size":"lead" - the single most important or exciting item today.
   - 3 to 5 cards are "feature". Everything else is "brief".
   - Put each card in a section. Use 4 to 7 sections, named like newspaper desks. "Front Page" must hold the lead plus 2 to 4 of the features (they run in columns on either side of the lead), whatever their beat. Use "Video Desk" for videos not on the front page, and "The X Wire" for posts. Invent the rest to fit the day (for example "Tech & AI", "Music", "Sports", "Range & Gear", "Workshop").
   - "sections" lists the section names in the order they should appear; "Front Page" first.

HARD RULES
- Every "url" must come from the wire copy above or from a page you actually saw in your search results or fetched. Never construct, guess or "fix" a URL.
- Videos must be youtube.com/watch?v=... (or youtu.be) links to the specific video, not a channel or search page.
- "kind" is exactly one of: "article", "video", "post".
- Second link, when you have one: if an "article" card is about a specific video (an interview, a trailer, a launch stream, an episode) and you saw that video's real YouTube link, put it in "video_url". If a "video" card has a good write-up or recap that you used, put that page in "article_url". The page then offers both "Read more" and "See the video". Same rule as every link: only URLs you actually saw.
- "published": ISO 8601 if you know it, otherwise omit it. Do not guess dates.
- Copy "image" through from the wire copy when the item has one; otherwise omit it (the app finds images itself).
- No duplicate stories: if three outlets cover one event, pick the best one.
- "tagline": a short, dry, one-line motto for today's edition (max 70 characters), in the spirit of a newspaper's front-page slogan.

OUTPUT
Inside every string, quotation marks are typographic: “like this” and ‘this’. Never put a straight double quote (") inside a string value - it breaks the JSON - and write paragraph breaks as \n\n, never as real line breaks.
Your final message must be the JSON object itself and nothing else - no summary of what you did, no "the edition is filed", no prose before or after, no code fence. A program parses your final message; anything but the JSON is a failed edition. Do not write the edition to a file or hand it to any tool.

{{"tagline":"...","sections":["Front Page","..."],"cards":[{{"kind":"article","size":"lead","section":"Front Page","headline":"...","dek":"...","story":"First paragraph.\n\nSecond paragraph.","source":"Publication or channel name","author":"optional","url":"https://...","video_url":"optional https://www.youtube.com/watch?v=...","article_url":"optional https://...","image":"optional https://...","published":"optional ISO 8601","why":"optional"}}]}}"#,
        videos = to_json(&videos),
        articles = to_json(&articles),
        lead_min = LEAD_CHARS.0,
        lead_max = LEAD_CHARS.1,
        feature_min = FEATURE_CHARS.0,
        feature_max = FEATURE_CHARS.1,
        brief_min = BRIEF_CHARS.0,
        brief_max = BRIEF_CHARS.1,
        post_min = POST_CHARS.0,
        post_max = POST_CHARS.1,
        min_cards = (max_cards / 2).max(8),
    )
}

// ---------------------------------------------------------------------- run

/// Runs Claude and streams status lines through `on_status(stage, message, detail)`.
///
/// If the installed CLI is old enough to reject one of the nice-to-have flags,
/// run once more with only the flags that have existed since headless mode did.
pub async fn run<F>(
    bin: &Path,
    cwd: &Path,
    prompt: &str,
    settings: &Settings,
    mut on_status: F,
) -> Result<EditorOutput, String>
where
    F: FnMut(&str, &str, Option<String>) + Send,
{
    match run_once(bin, cwd, prompt, settings, false, &mut on_status).await {
        Err(e) if looks_like_rejected_flag(&e) => {
            on_status("research", "Claude's at the desk", Some("Older CLI - retrying with basic options".into()));
            run_once(bin, cwd, prompt, settings, true, &mut on_status).await
        }
        other => other,
    }
}

pub fn looks_like_rejected_flag(err: &str) -> bool {
    let e = err.to_lowercase();
    ["unknown option", "unknown argument", "unexpected argument", "unrecognized option", "unrecognized argument"]
        .iter()
        .any(|needle| e.contains(needle))
}

async fn run_once<F>(
    bin: &Path,
    cwd: &Path,
    prompt: &str,
    settings: &Settings,
    minimal: bool,
    on_status: &mut F,
) -> Result<EditorOutput, String>
where
    F: FnMut(&str, &str, Option<String>) + Send,
{
    let env = paths::shell_env().await;

    let mut cmd = tokio::process::Command::new(bin);
    paths::quiet_async(&mut cmd);
    cmd.arg("-p")
        .args(["--output-format", "stream-json"])
        .arg("--verbose")
        .args(["--allowedTools", "WebSearch,WebFetch"]);
    if !minimal {
        // Token-level events, so "Building new Daily brief" shows up when the
        // writing starts rather than when it ends.
        cmd.arg("--include-partial-messages")
            .args(["--disallowedTools", "Bash,Edit,Write,NotebookEdit"])
            // Ignore whatever MCP servers are configured: faster start, less context.
            .arg("--strict-mcp-config")
            .args(["--max-turns", "60"]);
        // Picking and writing up the news needs judgement, not minutes of
        // deliberation: at the default effort the model spends ~2 silent
        // minutes thinking before it writes a word.
        let effort = settings.claude_effort.trim();
        if !effort.is_empty() && effort != "default" {
            cmd.args(["--effort", effort]);
        }
    }
    // Writing up the news doesn't need the biggest model, and the smaller one is
    // faster and lighter on the plan. settings.json: claudeModel = "opus" (etc.)
    // to change it, or "default" to use whatever the CLI is set to.
    match settings.claude_model.trim() {
        "" => {
            cmd.args(["--model", "sonnet"]);
        }
        "default" => {}
        other => {
            cmd.args(["--model", other]);
        }
    }
    cmd.current_dir(cwd)
        .env("PATH", &env.path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Couldn't start the Claude CLI at {}: {e}", bin.display()))?;

    // Prompt goes in on stdin: no argument-length or quoting problems.
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|e| format!("Couldn't send the prompt to Claude: {e}"))?;
        drop(stdin);
    }

    let stdout = child.stdout.take().ok_or("no stdout from Claude")?;
    let stderr = child.stderr.take().ok_or("no stderr from Claude")?;

    // Drain stderr in the background so a chatty CLI can't fill the pipe and stall.
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut tail: Vec<String> = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            if !l.trim().is_empty() {
                tail.push(l);
                if tail.len() > 12 {
                    tail.remove(0);
                }
            }
        }
        tail.join("\n")
    });

    let deadline = Instant::now() + Duration::from_secs(settings.claude_timeout_secs);
    let mut lines = BufReader::new(stdout).lines();

    let mut searches = 0u32;
    let mut pages_read = 0u32;
    let mut session_id: Option<String> = None;
    let mut result_text: Option<String> = None;
    let mut result_error: Option<String> = None;
    let mut cost_usd: Option<f64> = None;
    let mut last_text = String::new();
    // Every text block the model produced, in order. The edition is supposed
    // to be the final message, but a model in a hurry sometimes files the JSON
    // and then adds a closing remark - which would otherwise be all we see.
    let mut texts: Vec<String> = Vec::new();
    let mut announced_writing = false;
    // RD_DEBUG_STREAM=/some/file dumps the raw event stream for debugging.
    let debug_path = std::env::var("RD_DEBUG_STREAM").ok().filter(|p| !p.is_empty());

    on_status("research", "Claude's at the desk", Some("Reading the wire copy".into()));

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = child.kill().await;
            return Err(format!(
                "Claude ran past the {}-second limit. Try Refresh again, or raise claudeTimeoutSecs in settings.json.",
                settings.claude_timeout_secs
            ));
        }
        let line = match tokio::time::timeout(remaining, lines.next_line()).await {
            Err(_) => continue, // loop re-checks the deadline
            Ok(Err(e)) => return Err(format!("Lost the connection to Claude: {e}")),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(l))) => l,
        };
        if let Some(path) = &debug_path {
            use std::io::Write as _;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(f, "{line}");
            }
        }
        let Ok(ev) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if session_id.is_none() {
            session_id = ev.get("session_id").and_then(|v| v.as_str()).map(|v| v.to_string());
        }
        match ev.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "assistant" => {
                let blocks = ev
                    .pointer("/message/content")
                    .and_then(|c| c.as_array())
                    .cloned()
                    .unwrap_or_default();
                for b in blocks {
                    match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                        "tool_use" => {
                            let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let input = b.get("input").cloned().unwrap_or(Value::Null);
                            // If the model tries to hand the edition to some tool
                            // (write a file, send a message) instead of replying
                            // with it, the copy is still an edition. Keep it.
                            if let Some(obj) = input.as_object() {
                                for v in obj.values() {
                                    if let Some(t) = v.as_str() {
                                        if t.contains("\"cards\"") {
                                            texts.push(t.to_string());
                                        }
                                    }
                                }
                            }
                            match name {
                                "WebSearch" => {
                                    searches += 1;
                                    let q = input.get("query").and_then(|q| q.as_str()).unwrap_or("");
                                    on_status("research", "Claude's researching", Some(wire::truncate(q, 90)));
                                }
                                "WebFetch" => {
                                    pages_read += 1;
                                    let u = input.get("url").and_then(|q| q.as_str()).unwrap_or("");
                                    on_status("research", "Claude's reading", Some(wire::host_label(u)));
                                }
                                _ => {}
                            }
                        }
                        "text" => {
                            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                                if !t.trim().is_empty() {
                                    last_text = t.to_string();
                                    texts.push(t.to_string());
                                    if !announced_writing && t.contains("\"cards\"") {
                                        announced_writing = true;
                                        on_status("writing", "Building new Daily brief", None);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Partial-message events: the first text delta after research
            // means the edition is being written.
            "stream_event" => {
                let is_text_delta = ev.pointer("/event/delta/type").and_then(|t| t.as_str()) == Some("text_delta");
                if is_text_delta && !announced_writing && (searches > 0 || pages_read > 0) {
                    announced_writing = true;
                    on_status("writing", "Building new Daily brief", Some("Writing the stories".into()));
                }
            }
            "result" => {
                cost_usd = ev.get("total_cost_usd").and_then(|c| c.as_f64());
                let is_error = ev.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
                let text = ev.get("result").and_then(|r| r.as_str()).unwrap_or("").to_string();
                if is_error {
                    let subtype = ev.get("subtype").and_then(|s| s.as_str()).unwrap_or("error");
                    result_error = Some(if text.is_empty() { subtype.to_string() } else { text });
                } else {
                    result_text = Some(text);
                }
                break;
            }
            _ => {}
        }
    }

    let status = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    let stderr_tail = tokio::time::timeout(Duration::from_secs(2), stderr_task)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_default();

    if let Some(e) = result_error {
        return Err(explain_failure(&e, &stderr_tail));
    }

    let text = match result_text {
        Some(t) if !t.trim().is_empty() => t,
        _ if !last_text.trim().is_empty() => last_text,
        _ => {
            let code = match status {
                Ok(Ok(s)) => s.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into()),
                _ => "unknown".into(),
            };
            return Err(explain_failure(&format!("Claude exited (code {code}) without an edition."), &stderr_tail));
        }
    };

    if !announced_writing {
        on_status("writing", "Building new Daily brief", None);
    }

    // The final message first, then earlier ones, newest to oldest: take the
    // first that really is an edition.
    let json = std::iter::once(&text)
        .chain(texts.iter().rev())
        .find_map(|t| extract_edition(t))
        .ok_or_else(|| {
            format!(
                "Claude replied, but not with an edition I could parse. It started: \"{}\"",
                wire::truncate(text.trim(), 160)
            )
        })?;

    let (tagline, sections, cards) = cards_from_json(&json);
    if cards.is_empty() {
        return Err("Claude returned an edition with no usable cards.".into());
    }

    Ok(EditorOutput { session_id, tagline, sections, cards, searches, pages_read, cost_usd })
}

/// Send under-length stories back to the same Claude session, once. The
/// research is still in its context, so this is a rewrite, not a re-run.
/// Best effort: on any failure the originals stand.
pub async fn lengthen_short_stories(bin: &Path, cwd: &Path, settings: &Settings, session_id: &str, cards: &mut [Card]) -> usize {
    let short = short_cards(cards);
    if short.is_empty() {
        return 0;
    }
    let items: Vec<Value> = short
        .iter()
        .map(|&i| {
            let c = &cards[i];
            serde_json::json!({
                "key": i.to_string(),
                "headline": c.headline,
                "size": if c.kind == "post" { "post" } else { c.size.as_str() },
                "current_characters": story_len(c),
                "minimum_characters": min_chars(c),
                "current_story": c.story.clone().unwrap_or_default(),
            })
        })
        .collect();
    let prompt = format!(
        "The copy desk bounced these stories: each is under its minimum length and leaves a hole in its column. Rewrite ONLY these, each to at least its minimum_characters (aim 15% over), in the same voice and paragraph style as before. Use what you already found in your research in this conversation - background, numbers, names, dates, what happens next. No new tool calls. Do not pad, repeat yourself, or invent anything; if you truly have nothing more, tighten what is there and get as close as you honestly can.\n\n{}\n\nYour reply must be ONLY a JSON object mapping each key to its new story text, paragraphs separated by a blank line: {{\"0\": \"...\"}}",
        serde_json::to_string_pretty(&items).unwrap_or_default()
    );

    let env = paths::shell_env().await;
    let mut cmd = tokio::process::Command::new(bin);
    paths::quiet_async(&mut cmd);
    cmd.arg("-p").args(["--resume", session_id]).args(["--output-format", "json"]);
    match settings.claude_model.trim() {
        "" => {
            cmd.args(["--model", "sonnet"]);
        }
        "default" => {}
        other => {
            cmd.args(["--model", other]);
        }
    }
    cmd.current_dir(cwd).env("PATH", &env.path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);

    let Ok(mut child) = cmd.spawn() else { return 0 };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(prompt.as_bytes()).await.is_err() {
            return 0;
        }
        drop(stdin);
    }
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(180), child.wait_with_output()).await else { return 0 };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(envelope) = extract_json(&stdout) else { return 0 };
    let reply = envelope.get("result").and_then(|r| r.as_str()).and_then(extract_json).unwrap_or(envelope);

    let mut fixed = 0;
    for &i in &short {
        let Some(text) = reply.get(i.to_string()).and_then(|v| v.as_str()) else { continue };
        let text = wire::truncate(&tidy_story(text), 3200);
        // Only ever take an improvement.
        if text.chars().count() > story_len(&cards[i]) {
            cards[i].story = Some(text);
            fixed += 1;
        }
    }
    fixed
}

fn explain_failure(msg: &str, stderr_tail: &str) -> String {
    let all = format!("{msg}\n{stderr_tail}").to_lowercase();
    if all.contains("/login") || all.contains("not logged in") || all.contains("invalid api key") || all.contains("authentication") {
        return "Claude CLI isn't signed in. Open PowerShell, run `claude`, then `/login`, and hit Refresh again.".into();
    }
    if all.contains("usage limit") || all.contains("rate limit") || all.contains("limit reached") {
        return format!("Claude says you've hit a usage limit: {}", wire::truncate(msg.trim(), 200));
    }
    let mut out = wire::truncate(msg.trim(), 300);
    if !stderr_tail.trim().is_empty() {
        out.push_str(&format!(" - {}", wire::truncate(stderr_tail.trim(), 300)));
    }
    out
}

// ------------------------------------------------------------------ parsing

/// Find the first JSON *object* in a blob of text. Handles a bare object,
/// a ```json fence, and prose or log lines around the object.
/// The first JSON object in a reply, forgiving the usual model slips.
pub fn extract_json(text: &str) -> Option<Value> {
    extract_json_where(text, &|_| true)
}

/// The edition in a reply: the object that has a "cards" array.
pub fn extract_edition(text: &str) -> Option<Value> {
    let is_edition = |v: &Value| v.get("cards").map(|c| c.is_array()).unwrap_or(false);
    extract_json_where(text, &is_edition).or_else(|| {
        // Still broken somewhere: keep every card that parses on its own
        // rather than losing the whole edition to one bad comma.
        salvage_cards(&repair_json(text))
    })
}

fn extract_json_where(text: &str, wanted: &dyn Fn(&Value) -> bool) -> Option<Value> {
    if let Some(v) = extract_json_strict(text, wanted) {
        return Some(v);
    }
    // Long stories are where the model slips: a straight double quote inside
    // a string ("a "not so short" film") or a raw line break. Mend and retry.
    extract_json_strict(&repair_json(text), wanted)
}

fn extract_json_strict(text: &str, wanted: &dyn Fn(&Value) -> bool) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        if v.is_object() && wanted(&v) {
            return Some(v);
        }
    }
    for (i, _) in t.match_indices('{') {
        let mut stream = serde_json::Deserializer::from_str(&t[i..]).into_iter::<Value>();
        if let Some(Ok(v)) = stream.next() {
            if v.is_object() && wanted(&v) {
                return Some(v);
            }
        }
    }
    None
}

/// Escape what JSON strings can't hold raw. A `"` inside a string only counts
/// as the closing quote when what follows looks like JSON structure
/// (`:` `}` `]`, end of text, or `,` and then the next string / object / array).
pub fn repair_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let next_non_space = |from: usize| -> Option<(usize, char)> {
        (from..chars.len()).find(|&j| !chars[j].is_whitespace()).map(|j| (j, chars[j]))
    };
    let mut out = String::with_capacity(text.len() + 64);
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !in_string {
            in_string = c == '"';
            out.push(c);
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                out.push(c);
                if let Some(&n) = chars.get(i + 1) {
                    out.push(n);
                    i += 1;
                }
            }
            '"' => {
                let closes = match next_non_space(i + 1) {
                    None => true,
                    Some((_, ':')) | Some((_, '}')) | Some((_, ']')) => true,
                    Some((j, ',')) => matches!(next_non_space(j + 1), Some((_, '"')) | Some((_, '{')) | Some((_, '['))),
                    _ => false,
                };
                if closes {
                    in_string = false;
                    out.push('"');
                } else {
                    out.push_str("\\\"");
                }
            }
            '\n' => out.push_str("\\n"),
            '\r' => {}
            '\t' => out.push(' '),
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Last resort: parse the objects inside "cards":[ ... ] one at a time.
fn salvage_cards(text: &str) -> Option<Value> {
    let start = text.find("\"cards\"")?;
    let open = start + text[start..].find('[')?;
    let bytes = text.as_bytes();
    let mut cards: Vec<Value> = Vec::new();
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    let mut object_start = None;
    for (offset, &b) in bytes[open + 1..].iter().enumerate() {
        let at = open + 1 + offset;
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    object_start = Some(at);
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = object_start.take() {
                        if let Ok(v) = serde_json::from_str::<Value>(&text[s..=at]) {
                            cards.push(v);
                        }
                    }
                }
            }
            b']' if depth == 0 => break,
            _ => {}
        }
    }
    if cards.is_empty() {
        return None;
    }
    // The head of the object ("tagline", "sections") usually parses fine once
    // the cards are cut away.
    let head_end = text[..start].rfind(',').unwrap_or(start);
    let head_start = text[..start].rfind('{').unwrap_or(0);
    let mut edition = serde_json::from_str::<Value>(&format!("{}}}", &text[head_start..head_end]))
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    edition["cards"] = Value::Array(cards);
    Some(edition)
}

fn is_web_url(u: &str) -> bool {
    url::Url::parse(u)
        .map(|p| matches!(p.scheme(), "http" | "https") && p.host_str().is_some())
        .unwrap_or(false)
}

/// Normalise paragraph breaks: any run of blank lines becomes exactly one,
/// single newlines inside a paragraph become spaces.
pub fn tidy_story(text: &str) -> String {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .map(|para| para.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|para| !para.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Lenient: one malformed card is dropped, not fatal.
pub fn cards_from_json(v: &Value) -> (Option<String>, Vec<String>, Vec<Card>) {
    let s = |o: &Value, k: &str| -> Option<String> {
        o.get(k)
            .and_then(|x| x.as_str())
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty() && x != "null")
    };

    let tagline = s(v, "tagline").map(|t| wire::truncate(&t, 90));

    let mut cards: Vec<Card> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let empty = vec![];
    for (n, c) in v.get("cards").and_then(|c| c.as_array()).unwrap_or(&empty).iter().enumerate() {
        let Some(url) = s(c, "url") else { continue };
        let Some(headline) = s(c, "headline") else { continue };
        let Ok(parsed) = url::Url::parse(&url) else { continue };
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            continue;
        }
        if !seen.insert(url.clone()) {
            continue;
        }

        let is_youtube = wire::youtube_id(&url).is_some();
        let is_post = crate::grok::is_post_url(&url);
        let kind = match s(c, "kind").as_deref() {
            _ if is_youtube => "video",
            _ if is_post => "post",
            Some("post") | Some("video") => "article", // claimed, but the link disagrees
            _ => "article",
        }
        .to_string();

        let kind_is_video = kind == "video";

        let size = match s(c, "size").as_deref() {
            Some("lead") => "lead",
            Some("feature") => "feature",
            _ => "brief",
        }
        .to_string();

        let image = s(c, "image").filter(|i| i.starts_with("http"));

        cards.push(Card {
            id: format!("c{n}"),
            kind,
            size,
            section: s(c, "section").unwrap_or_else(|| "Front Page".into()),
            headline: wire::truncate(&headline, 140),
            dek: s(c, "dek").map(|d| wire::truncate(&d, 320)).unwrap_or_default(),
            story: s(c, "story").map(|t| wire::truncate(&tidy_story(&t), 3200)),
            source: s(c, "source").unwrap_or_else(|| wire::host_label(&url)),
            author: s(c, "author"),
            video_url: if kind_is_video { None } else { s(c, "video_url").filter(|u| wire::youtube_id(u).is_some()) },
            article_url: if kind_is_video {
                s(c, "article_url").filter(|u| is_web_url(u) && wire::youtube_id(u).is_none())
            } else {
                None
            },
            url,
            image,
            published: s(c, "published").and_then(|p| wire::normalize_date(&p)),
            why: s(c, "why").map(|w| wire::truncate(&w, 120)),
        });
    }

    // Exactly one lead: keep the first, demote extras; promote if none.
    let mut lead_seen = false;
    for c in cards.iter_mut() {
        if c.size == "lead" {
            if lead_seen {
                c.size = "feature".into();
            }
            lead_seen = true;
        }
    }
    if !lead_seen {
        if let Some(i) = cards.iter().position(|c| c.size == "feature").or(if cards.is_empty() { None } else { Some(0) }) {
            cards[i].size = "lead".into();
            cards[i].section = "Front Page".into();
        }
    }

    // Section order: what the editor asked for, then anything it forgot.
    let mut sections: Vec<String> = v
        .get("sections")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default();
    for c in &cards {
        if !sections.contains(&c.section) {
            sections.push(c.section.clone());
        }
    }
    sections.retain(|sec| cards.iter().any(|c| &c.section == sec));
    if let Some(pos) = sections.iter().position(|x| x == "Front Page") {
        let fp = sections.remove(pos);
        sections.insert(0, fp);
    }

    (tagline, sections, cards)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_messy_replies() {
        assert!(extract_json(r#"{"a":1}"#).is_some());
        assert!(extract_json("Here you go:\n```json\n{\"a\":{\"b\":[1,2]}}\n```\nEnjoy!").is_some());
        assert!(extract_json("log {not json} then {\"ok\":true} trailing }").unwrap()["ok"].as_bool().unwrap());
        assert!(extract_json("no json here").is_none());
        assert!(extract_json("[1,2,3]").is_none());
    }

    #[test]
    fn builds_cards_leniently() {
        let v: Value = serde_json::from_str(
            r#"{"tagline":"All the news that fits","sections":["Video Desk","Front Page","Ghost Section"],
            "cards":[
              {"kind":"article","size":"feature","section":"Front Page","headline":"A","dek":"d","source":"S","url":"https://a.test/1"},
              {"kind":"article","size":"lead","section":"Front Page","headline":"B","dek":"d","url":"https://www.b.test/2","published":"Thu, 17 Sep 2026 14:05:00 GMT"},
              {"kind":"article","size":"lead","section":"Tech","headline":"Second lead","dek":"d","url":"https://c.test/3"},
              {"kind":"article","size":"brief","section":"Video Desk","headline":"Vid","url":"https://youtu.be/abcdefghijk"},
              {"kind":"video","size":"brief","section":"Video Desk","headline":"Not a video","url":"https://d.test/v"},
              {"kind":"article","headline":"dupe","url":"https://a.test/1"},
              {"kind":"article","headline":"bad url","url":"javascript:alert(1)"},
              {"kind":"article","url":"https://e.test/no-headline"},
              "not an object"
            ]}"#,
        )
        .unwrap();
        let (tagline, sections, cards) = cards_from_json(&v);
        assert_eq!(tagline.as_deref(), Some("All the news that fits"));
        assert_eq!(cards.len(), 5);
        assert_eq!(cards.iter().filter(|c| c.size == "lead").count(), 1);
        assert_eq!(cards[1].size, "lead");
        assert_eq!(cards[1].source, "b.test");
        assert_eq!(cards[1].published.as_deref(), Some("2026-09-17T14:05:00Z"));
        assert_eq!(cards[2].size, "feature");
        assert_eq!(cards[3].kind, "video");
        assert_eq!(cards[4].kind, "article");
        assert_eq!(sections, vec!["Front Page", "Video Desk", "Tech"]);
    }

    #[test]
    fn keeps_story_paragraphs() {
        let v: Value = serde_json::from_str(
            r#"{"cards":[{"headline":"A","size":"lead","url":"https://a.test/1","story":"First para\nwraps here.\n\n\n  Second para.  \r\n\r\nThird."},{"headline":"B","url":"https://a.test/2"}]}"#,
        )
        .unwrap();
        let (_, _, cards) = cards_from_json(&v);
        assert_eq!(cards[0].story.as_deref(), Some("First para wraps here.\n\nSecond para.\n\nThird."));
        assert_eq!(cards[1].story, None);
    }

    #[test]
    fn second_links_are_validated() {
        let v: Value = serde_json::from_str(
            r#"{"cards":[
              {"headline":"Article about a video","size":"lead","url":"https://a.test/1","video_url":"https://www.youtube.com/watch?v=abcdefghijk","article_url":"https://ignored.test/x"},
              {"headline":"Article, bogus video link","url":"https://a.test/2","video_url":"https://a.test/not-youtube"},
              {"headline":"Video with a recap","url":"https://youtu.be/abcdefghijk","article_url":"https://recap.test/ep","video_url":"https://www.youtube.com/watch?v=zzzzzzzzzzz"},
              {"headline":"Video, bad recap link","url":"https://www.youtube.com/watch?v=bbbbbbbbbbb","article_url":"javascript:alert(1)"}
            ]}"#,
        )
        .unwrap();
        let (_, _, cards) = cards_from_json(&v);
        assert_eq!(cards[0].video_url.as_deref(), Some("https://www.youtube.com/watch?v=abcdefghijk"));
        assert_eq!(cards[0].article_url, None);
        assert_eq!(cards[1].video_url, None);
        assert_eq!(cards[2].kind, "video");
        assert_eq!(cards[2].article_url.as_deref(), Some("https://recap.test/ep"));
        assert_eq!(cards[2].video_url, None);
        assert_eq!(cards[3].article_url, None);
    }

    #[test]
    fn finds_stories_that_leave_a_hole() {
        let mk = |kind: &str, size: &str, chars: usize| Card {
            id: "x".into(), kind: kind.into(), size: size.into(), section: "S".into(), headline: "H".into(), dek: String::new(),
            story: Some("a".repeat(chars)), source: "s".into(), author: None, url: "https://a.test".into(),
            video_url: None, article_url: None, image: None, published: None, why: None,
        };
        let cards = vec![
            mk("article", "lead", 1600),    // fine
            mk("article", "brief", 200),    // short
            mk("article", "brief", 480),    // within the 10% grace
            mk("post", "brief", 100),       // short
            mk("article", "feature", 700),  // short
        ];
        assert_eq!(short_cards(&cards), vec![1, 3, 4]);
    }

    #[test]
    fn promotes_a_lead_when_none_given() {
        let v: Value = serde_json::from_str(
            r#"{"cards":[{"headline":"A","url":"https://a.test/1","section":"Tech"},{"headline":"B","size":"feature","url":"https://a.test/2","section":"Tech"}]}"#,
        )
        .unwrap();
        let (_, sections, cards) = cards_from_json(&v);
        assert_eq!(cards[1].size, "lead");
        assert_eq!(cards[1].section, "Front Page");
        assert_eq!(sections[0], "Front Page");
    }

    /// Real end-to-end run against the installed `claude` CLI (uses web search
    /// and your Claude plan). Not part of the normal test run:
    ///   cargo test --lib e2e_claude -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn e2e_claude_builds_an_edition() {
        let interests: Vec<Interest> = serde_json::from_str::<crate::model::InterestsFile>(include_str!("../resources/default-interests.json"))
            .unwrap()
            .interests
            .into_iter()
            .take(5)
            .collect();
        let prompt = build_prompt(&interests, &[], &[], Some("Grok CLI not installed"), "today", 12, "Test Daily");
        let bin = paths::find_bin("claude", "").await.expect("claude on PATH");
        let cwd = std::env::temp_dir().join("my-daily-newspaper-e2e");
        std::fs::create_dir_all(&cwd).unwrap();
        let settings = Settings { claude_timeout_secs: 600, ..Default::default() };
        let t0 = std::time::Instant::now();
        let out = run(&bin, &cwd, &prompt, &settings, move |stage, msg, detail| {
            println!("{:>4}s [{stage}] {msg} {}", t0.elapsed().as_secs(), detail.unwrap_or_default());
        })
        .await
        .expect("edition");
        let mut out = out;
        let before: Vec<usize> = out.cards.iter().map(story_len).collect();
        let short_before = short_cards(&out.cards).len();
        let fixed = match &out.session_id {
            Some(sid) => lengthen_short_stories(&bin, &cwd, &settings, sid, &mut out.cards).await,
            None => 0,
        };
        println!("{:>4}s LENGTHS before: {:?}", t0.elapsed().as_secs(), before);
        println!("      short before: {short_before}, rewritten: {fixed}, short after: {}", short_cards(&out.cards).len());
        println!("tagline: {:?}\nsections: {:?}\nsearches: {} pages: {} cost: {:?}", out.tagline, out.sections, out.searches, out.pages_read, out.cost_usd);
        for c in &out.cards {
            println!("\n- [{}/{}/{}] {} <{}> ({})", c.size, c.kind, c.section, c.headline, c.url, c.source);
            println!("  DEK: {}", c.dek);
            let story = c.story.clone().unwrap_or_default();
            println!("  STORY ({} chars, min {}): {}", story.chars().count(), min_chars(c), story.replace("\n\n", "\n  ¶ "));
        }
        assert!(out.cards.len() >= 5);
        assert_eq!(out.cards.iter().filter(|c| c.size == "lead").count(), 1);
    }

    #[test]
    fn mends_straight_quotes_inside_stories() {
        let broken = r#"{"tagline":"t","sections":["Front Page"],"cards":[{"kind":"video","size":"lead","section":"Front Page","headline":"Drake's "FOMO" lands","dek":"d","story":"It premiered as a "not so short film" running an hour, he said "no", then left.
Second line.","source":"S","url":"https://example.com/a"}]}"#;
        assert!(serde_json::from_str::<Value>(broken).is_err());
        let v = extract_edition(broken).expect("mended");
        let story = v["cards"][0]["story"].as_str().unwrap();
        assert!(story.contains("\"not so short film\""));
        assert!(story.contains("said \"no\", then left."));
        assert_eq!(v["cards"][0]["headline"], "Drake's \"FOMO\" lands");
        assert_eq!(v["cards"][0]["url"], "https://example.com/a");
    }

    #[test]
    fn repair_leaves_good_json_alone() {
        let good = r#"{"a":"x \"quoted\" y","b":["1","2"],"c":{"d":"e"}}"#;
        assert_eq!(repair_json(good), good);
    }

    #[test]
    fn salvages_the_cards_that_parse() {
        // second card has a stray bare word: unfixable, so it is dropped alone
        let text = r#"{"tagline":"hello","sections":["Front Page"],"cards":[{"headline":"one","url":"https://example.com/1"},{"headline":"two" oops,"url":"https://example.com/2"},{"headline":"three","url":"https://example.com/3"}]}"#;
        let v = extract_edition(text).expect("salvaged");
        let cards = v["cards"].as_array().unwrap();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1]["headline"], "three");
        assert_eq!(v["tagline"], "hello");
    }

    /// RD_REPLY=/path/to/reply.txt cargo test --lib replay_reply -- --ignored --nocapture
    #[test]
    #[ignore]
    fn replay_reply() {
        let text = std::fs::read_to_string(std::env::var("RD_REPLY").expect("RD_REPLY")).unwrap();
        println!("strict json ok: {}", serde_json::from_str::<Value>(text.trim()).is_ok());
        let v = extract_edition(&text).expect("an edition");
        let (_, sections, cards) = cards_from_json(&v);
        println!("sections: {sections:?}");
        for c in &cards {
            println!("{:8} {:8} {:5} chars  {}", c.size, c.kind, story_len(c), c.headline);
        }
        println!("short: {:?}", short_cards(&cards));
    }

    #[test]
    fn spots_a_cli_that_rejected_a_flag() {
        assert!(looks_like_rejected_flag("Claude exited (code 1) without an edition. - error: unknown option '--include-partial-messages'"));
        assert!(looks_like_rejected_flag("error: unexpected argument '--strict-mcp-config' found"));
        assert!(!looks_like_rejected_flag("Claude CLI isn't signed in."));
        assert!(!looks_like_rejected_flag("Claude ran past the 600-second limit."));
    }

    #[test]
    fn prompt_mentions_beats_and_wire() {
        let interests = vec![Interest { id: "a".into(), name: "Rockets".into(), kind: "topic".into(), value: "".into(), notes: "big ones".into(), enabled: true },
                             Interest { id: "b".into(), name: "Hidden".into(), kind: "topic".into(), value: "".into(), notes: "".into(), enabled: false }];
        let wire = vec![WireItem { kind: "video".into(), title: "Launch".into(), url: "https://www.youtube.com/watch?v=abcdefghijk".into(), source: "YouTube".into(), beat: "Rockets".into(), ..Default::default() }];
        let p = build_prompt(&interests, &wire, &[], Some("Grok CLI not installed"), "Friday, September 18, 2026", 30, "Priya\u{2019}s Daily");
        assert!(p.contains("editor-in-chief of \"Priya\u{2019}s Daily\""));
        assert!(!p.contains("Richard"));
        assert!(p.contains("brief    520-800 characters"));
        assert!(p.contains("1. Rockets [topic] - big ones"));
        assert!(!p.contains("Hidden"));
        assert!(p.contains("watch?v=abcdefghijk"));
        assert!(p.contains("Grok CLI not installed"));
        assert!(p.contains("15 to 30"));
    }
}
