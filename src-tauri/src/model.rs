//! Shared data types: interests, settings, wire copy, editions, status events.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- interests

/// One thing the reader cares about. `kind` tells the wire service what to do
/// with it; every interest (whatever its kind) is also handed to the editors.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Interest {
    pub id: String,
    pub name: String,
    /// "topic" | "person" | "youtube_channel" | "x_account"
    #[serde(default = "default_kind")]
    pub kind: String,
    /// youtube_channel: "@handle", a channel URL, or a "UC..." channel id.
    /// x_account: "@handle". topic/person: optional custom news search query.
    #[serde(default)]
    pub value: String,
    /// Free-text guidance for the editor ("long-form interviews only").
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterestsFile {
    #[serde(default = "default_version")]
    pub version: u32,
    pub interests: Vec<Interest>,
}

fn default_kind() -> String {
    "topic".into()
}
fn default_true() -> bool {
    true
}
fn default_version() -> u32 {
    1
}

// ----------------------------------------------------------------- settings

/// Optional `settings.json` in the app data folder. Every field has a default,
/// so the file does not need to exist.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Absolute path to the `claude` binary. Empty = auto-detect.
    pub claude_bin: String,
    /// Absolute path to the `grok` binary. Empty = auto-detect.
    pub grok_bin: String,
    /// Model for the editor. Empty = "sonnet" (fast, light on the plan).
    /// "opus" etc. to change it; "default" = whatever the CLI is set to.
    pub claude_model: String,
    /// `--effort` for the editor: "low" (default, fastest), "medium", "high".
    /// "default" = leave it to the CLI.
    pub claude_effort: String,
    /// Use the Grok Build CLI for the X wire when it is installed.
    pub use_grok: bool,
    /// Reader popups float above other windows.
    pub reader_always_on_top: bool,
    /// Upper bound on cards per edition.
    pub max_cards: usize,
    /// Hard stop for the Claude run, seconds.
    pub claude_timeout_secs: u64,
    /// Hard stop for the Grok run, seconds.
    pub grok_timeout_secs: u64,
    /// MLB team for the score bug in the top bar. 119 = Dodgers. 0 = off.
    pub mlb_team_id: u32,
    /// Block ads in the reader windows (never on YouTube or X).
    pub block_ads: bool,

    // --- whose paper this is ---
    /// First name for the masthead ("<Name>'s Daily"). Empty = the name given
    /// to the builder script, if any.
    pub owner_name: String,
    /// City for the dateline. Empty = no city.
    pub city: String,
    /// The welcome pages have been seen (or skipped).
    pub onboarded: bool,

    // --- the paper edition ---
    /// Print automatically after the scheduled morning refresh.
    pub print_daily: bool,
    /// Printer name as `lpstat -e` lists it. Empty = the system default.
    pub printer: String,
    /// Colour photos on paper. Default is black & white: newsprint, less ink.
    pub print_color: bool,
    /// A QR code per story, since paper can't be clicked.
    pub print_qr: bool,
    pub print_duplex: bool,
    /// Never send more than this many pages to the printer. 0 = no cap.
    pub print_max_pages: u32,
    /// Chromium-family browser used to make the PDF. Empty = auto-detect.
    pub chrome_bin: String,
    /// Windows: SumatraPDF.exe, which sends the PDF to the printer. Empty = auto-detect.
    pub sumatra_bin: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            claude_bin: String::new(),
            grok_bin: String::new(),
            claude_model: String::new(),
            claude_effort: "low".into(),
            use_grok: true,
            reader_always_on_top: true,
            max_cards: 30,
            claude_timeout_secs: 600,
            grok_timeout_secs: 240,
            mlb_team_id: 119,
            block_ads: true,
            owner_name: String::new(),
            city: "Los Angeles".into(),
            onboarded: false,
            print_daily: false,
            printer: String::new(),
            print_color: false,
            print_qr: true,
            print_duplex: true,
            print_max_pages: 8,
            chrome_bin: String::new(),
            sumatra_bin: String::new(),
        }
    }
}

// ---------------------------------------------------------------- wire copy

/// A raw item from a feed or from Grok, before the editor touches it.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WireItem {
    /// "video" | "article" | "post"
    pub kind: String,
    pub title: String,
    pub url: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Name of the interest that produced this item.
    pub beat: String,
}

// ------------------------------------------------------------------ edition

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub id: String,
    /// "article" | "video" | "post"
    pub kind: String,
    /// "lead" | "feature" | "brief"
    pub size: String,
    pub section: String,
    pub headline: String,
    /// One-sentence standfirst. For posts: the post's own text, verbatim.
    pub dek: String,
    /// The piece itself, written by the editor so the page can be read without
    /// clicking through. Paragraphs are separated by a blank line. Older
    /// editions don't have it.
    #[serde(default)]
    pub story: Option<String>,
    pub source: String,
    #[serde(default)]
    pub author: Option<String>,
    pub url: String,
    /// An article about a specific video: the video itself ("See the video").
    #[serde(default)]
    pub video_url: Option<String>,
    /// A video with a good write-up: that page ("Read more").
    #[serde(default)]
    pub article_url: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct EditionStats {
    pub duration_secs: u64,
    pub searches: u32,
    pub pages_read: u32,
    pub wire_items: usize,
    pub x_posts_from_grok: usize,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Edition {
    /// Local date, YYYY-MM-DD.
    pub date: String,
    /// RFC 3339 timestamp.
    pub generated_at: String,
    pub edition_no: u32,
    #[serde(default)]
    pub tagline: Option<String>,
    /// Section names in display order.
    pub sections: Vec<String>,
    pub cards: Vec<Card>,
    /// Things the reader should know about how this edition was built
    /// ("X wire: Grok CLI not found, Claude covered it").
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub stats: EditionStats,
}

// ------------------------------------------------------------------- status

#[derive(Serialize, Clone, Debug)]
pub struct Status {
    /// "wire" | "grok" | "research" | "writing" | "content" | "done" | "error"
    pub stage: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub at: i64,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub claude_path: Option<String>,
    pub grok_path: Option<String>,
    pub data_dir: String,
}

/// Whose paper this is, as the front end needs it.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub owner_name: String,
    /// "Sam's Daily"
    pub paper_name: String,
    pub city: String,
    pub mlb_team_id: u32,
    pub onboarded: bool,
}
