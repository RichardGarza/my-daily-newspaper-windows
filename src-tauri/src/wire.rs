//! The wire service: deterministic feeds, no AI involved.
//!
//! YouTube channel feeds and news search feeds are free, fast and - unlike a
//! language model - cannot invent a link. Everything here is best-effort: a
//! dead feed costs us some wire copy, never the edition.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::join_all;
use regex::Regex;

use crate::model::{Interest, WireItem};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 Edg/126.0.0.0";

const VIDEO_MAX_AGE_DAYS: i64 = 10;
const NEWS_MAX_AGE_DAYS: i64 = 5;
const VIDEOS_PER_CHANNEL: usize = 4;
const NEWS_PER_TOPIC: usize = 6;

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::limited(6))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

pub struct WireResult {
    pub items: Vec<WireItem>,
    pub notes: Vec<String>,
}

/// Pull every feed implied by the enabled interests, concurrently.
pub async fn gather(client: &reqwest::Client, interests: &[Interest], data_dir: PathBuf) -> WireResult {
    let cache_path = data_dir.join("channel-cache.json");
    let cache: HashMap<String, String> = std::fs::read_to_string(&cache_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    let tasks = interests.iter().filter(|i| i.enabled).map(|interest| {
        let client = client.clone();
        let cached = cache.get(interest.value.trim()).cloned();
        async move {
            match interest.kind.as_str() {
                "youtube_channel" => youtube_for(&client, interest, cached).await,
                "topic" | "person" => news_for(&client, interest).await,
                _ => FeedOutcome::default(),
            }
        }
    });

    let outcomes = join_all(tasks).await;

    let mut items = Vec::new();
    let mut notes = Vec::new();
    let mut new_cache = cache.clone();
    for o in outcomes {
        items.extend(o.items);
        if let Some(n) = o.note {
            notes.push(n);
        }
        if let Some((k, v)) = o.resolved {
            new_cache.insert(k, v);
        }
    }
    if new_cache.len() != cache.len() {
        if let Ok(text) = serde_json::to_string_pretty(&new_cache) {
            let _ = std::fs::write(&cache_path, text);
        }
    }

    // De-dupe by URL, keep first (interests are in priority order).
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(i.url.clone()));

    WireResult { items, notes }
}

#[derive(Default)]
struct FeedOutcome {
    items: Vec<WireItem>,
    note: Option<String>,
    /// (interest.value, channel id) to remember.
    resolved: Option<(String, String)>,
}

// ------------------------------------------------------------------ YouTube

async fn youtube_for(client: &reqwest::Client, interest: &Interest, cached: Option<String>) -> FeedOutcome {
    let key = interest.value.trim().to_string();
    let (channel_id, fresh) = match cached {
        Some(id) => (Some(id), false),
        None => (resolve_channel_id(client, &key).await, true),
    };
    let Some(channel_id) = channel_id else {
        return FeedOutcome {
            note: Some(format!(
                "Couldn't find the YouTube channel \"{}\" ({}). Fix the handle in Edit Interests.",
                interest.name, key
            )),
            ..Default::default()
        };
    };

    let url = format!("https://www.youtube.com/feeds/videos.xml?channel_id={channel_id}");
    let xml = match get_text(client, &url).await {
        Ok(x) => x,
        Err(e) => {
            return FeedOutcome {
                note: Some(format!("YouTube feed for \"{}\" failed: {e}", interest.name)),
                ..Default::default()
            }
        }
    };

    let items = parse_feed(&xml)
        .into_iter()
        .filter(|e| within_days(e.published.as_deref(), VIDEO_MAX_AGE_DAYS))
        .take(VIDEOS_PER_CHANNEL)
        .filter_map(|e| {
            let id = e.video_id.clone().or_else(|| youtube_id(&e.link))?;
            Some(WireItem {
                kind: "video".into(),
                title: e.title,
                url: format!("https://www.youtube.com/watch?v={id}"),
                source: "YouTube".into(),
                author: e.author,
                published: e.published,
                // Descriptions list guests and topics: the raw material for
                // writing up a video nobody has watched yet.
                summary: e.summary.map(|s| truncate(&s, 700)),
                image: Some(youtube_thumb(&id)),
                beat: interest.name.clone(),
            })
        })
        .collect();

    FeedOutcome {
        items,
        note: None,
        resolved: fresh.then_some((key, channel_id)),
    }
}

/// "@handle", a channel URL, or a raw "UC..." id -> channel id.
pub async fn resolve_channel_id(client: &reqwest::Client, value: &str) -> Option<String> {
    static ID: OnceLock<Regex> = OnceLock::new();
    let id_re = ID.get_or_init(|| Regex::new(r"UC[0-9A-Za-z_-]{22}").unwrap());

    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    if let Some(m) = id_re.find(v) {
        if v.starts_with("UC") || v.contains("/channel/") {
            return Some(m.as_str().to_string());
        }
    }
    let page_url = if v.starts_with("http://") || v.starts_with("https://") {
        v.to_string()
    } else {
        format!("https://www.youtube.com/@{}", v.trim_start_matches('@'))
    };

    let html = client
        .get(&page_url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Cookie", "CONSENT=YES+1; SOCS=CAI")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    channel_id_from_html(&html)
}

pub fn channel_id_from_html(html: &str) -> Option<String> {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            r#"rel="canonical"\s+href="https://www\.youtube\.com/channel/(UC[0-9A-Za-z_-]{22})""#,
            r#""externalId":"(UC[0-9A-Za-z_-]{22})""#,
            r#"feeds/videos\.xml\?channel_id=(UC[0-9A-Za-z_-]{22})"#,
            r#""channelId":"(UC[0-9A-Za-z_-]{22})""#,
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    });
    patterns
        .iter()
        .find_map(|re| re.captures(html).map(|c| c[1].to_string()))
}

pub fn youtube_id(url: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?:youtube\.com/(?:watch\?(?:[^#]*&)?v=|shorts/|live/|embed/)|youtu\.be/)([0-9A-Za-z_-]{11})").unwrap()
    });
    re.captures(url).map(|c| c[1].to_string())
}

pub fn youtube_thumb(id: &str) -> String {
    format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg")
}

// --------------------------------------------------------------------- news

async fn news_for(client: &reqwest::Client, interest: &Interest) -> FeedOutcome {
    let query = if interest.value.trim().is_empty() {
        interest.name.clone()
    } else {
        interest.value.trim().to_string()
    };

    let mut entries = Vec::new();
    let mut errors = Vec::new();

    // Bing first: its links unwrap to the publisher's real URL.
    let bing = url::Url::parse_with_params(
        "https://www.bing.com/news/search",
        &[("q", query.as_str()), ("format", "rss"), ("setlang", "en-US"), ("cc", "US")],
    );
    if let Ok(u) = bing {
        match get_text(client, u.as_str()).await {
            Ok(xml) => entries = parse_feed(&xml),
            Err(e) => errors.push(format!("bing: {e}")),
        }
    }

    // Google News as the backup. Its links go through a redirect page, which
    // the reader window handles fine.
    if entries.is_empty() {
        let q = format!("{query} when:{NEWS_MAX_AGE_DAYS}d");
        let google = url::Url::parse_with_params(
            "https://news.google.com/rss/search",
            &[("q", q.as_str()), ("hl", "en-US"), ("gl", "US"), ("ceid", "US:en")],
        );
        if let Ok(u) = google {
            match get_text(client, u.as_str()).await {
                Ok(xml) => entries = parse_feed(&xml),
                Err(e) => errors.push(format!("google: {e}")),
            }
        }
    }

    if entries.is_empty() && !errors.is_empty() {
        return FeedOutcome {
            note: Some(format!("News wire for \"{}\" failed ({})", interest.name, errors.join("; "))),
            ..Default::default()
        };
    }

    let items = entries
        .into_iter()
        .filter(|e| within_days(e.published.as_deref(), NEWS_MAX_AGE_DAYS))
        .filter(|e| e.link.starts_with("http"))
        .take(NEWS_PER_TOPIC)
        .map(|e| {
            let url = unwrap_redirect(&e.link);
            let source = e
                .source
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| host_label(&url));
            let mut title = e.title.clone();
            let suffix = format!(" - {source}");
            if title.ends_with(&suffix) {
                title.truncate(title.len() - suffix.len());
            }
            WireItem {
                kind: if youtube_id(&url).is_some() { "video".into() } else { "article".into() },
                title,
                url,
                source,
                author: None,
                published: e.published,
                summary: e.summary.map(|s| truncate(&s, 260)),
                image: e.image,
                beat: interest.name.clone(),
            }
        })
        .collect();

    FeedOutcome { items, ..Default::default() }
}

/// Bing wraps links as .../apiclick.aspx?...&url=<real>. Unwrap when we can.
pub fn unwrap_redirect(link: &str) -> String {
    if let Ok(u) = url::Url::parse(link) {
        let host = u.host_str().unwrap_or("");
        if host.ends_with("bing.com") && u.path().contains("apiclick") {
            if let Some((_, real)) = u.query_pairs().find(|(k, _)| k == "url") {
                if real.starts_with("http") {
                    return real.to_string();
                }
            }
        }
    }
    link.to_string()
}

pub fn host_label(link: &str) -> String {
    url::Url::parse(link)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
        .unwrap_or_default()
}

// ------------------------------------------------------------- feed parsing

#[derive(Debug, Default, Clone)]
pub struct FeedEntry {
    pub title: String,
    pub link: String,
    pub published: Option<String>,
    pub summary: Option<String>,
    pub source: Option<String>,
    pub image: Option<String>,
    pub video_id: Option<String>,
    pub author: Option<String>,
}

/// Parses both RSS 2.0 (`item`) and Atom (`entry`), matching on local tag
/// names so namespace prefixes (yt:, media:, News:) don't matter.
pub fn parse_feed(xml: &str) -> Vec<FeedEntry> {
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = roxmltree::Document::parse_with_options(xml.trim_start_matches('\u{feff}'), opts) else {
        return vec![];
    };

    let mut out = Vec::new();
    for node in doc.descendants().filter(|n| {
        n.is_element() && matches!(n.tag_name().name(), "item" | "entry")
    }) {
        let mut e = FeedEntry::default();
        for child in node.children().filter(|c| c.is_element()) {
            let name = child.tag_name().name().to_ascii_lowercase();
            let text = child.text().map(|t| t.trim().to_string()).unwrap_or_default();
            match name.as_str() {
                "title" => e.title = clean_text(&text),
                "link" => {
                    if let Some(href) = child.attribute("href") {
                        let rel = child.attribute("rel").unwrap_or("alternate");
                        if rel == "alternate" || e.link.is_empty() {
                            e.link = href.to_string();
                        }
                    } else if !text.is_empty() {
                        e.link = text;
                    }
                }
                "pubdate" | "published" | "date" => e.published = normalize_date(&text).or(Some(text)),
                "updated" => {
                    if e.published.is_none() {
                        e.published = normalize_date(&text);
                    }
                }
                "description" | "summary" => {
                    if !text.is_empty() {
                        e.summary = Some(clean_text(&text));
                    }
                }
                "source" => e.source = Some(clean_text(&text)),
                "image" => {
                    if text.starts_with("http") {
                        e.image = Some(text);
                    }
                }
                "videoid" => e.video_id = Some(text),
                "author" => {
                    let nm = child
                        .children()
                        .find(|c| c.is_element() && c.tag_name().name() == "name")
                        .and_then(|c| c.text())
                        .map(|t| t.trim().to_string());
                    e.author = nm.or(if text.is_empty() { None } else { Some(text) });
                }
                "group" => {
                    // media:group (YouTube)
                    for g in child.children().filter(|c| c.is_element()) {
                        match g.tag_name().name() {
                            "description" => {
                                if let Some(t) = g.text() {
                                    let t = clean_text(t);
                                    if !t.is_empty() {
                                        e.summary = Some(t);
                                    }
                                }
                            }
                            "thumbnail" => {
                                if let Some(u) = g.attribute("url") {
                                    e.image = Some(u.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if !e.title.is_empty() && !e.link.is_empty() {
            out.push(e);
        }
    }
    out
}

/// Any common feed date -> RFC 3339 UTC.
pub fn normalize_date(s: &str) -> Option<String> {
    parse_date(s).map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

pub fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_rfc2822(s))
        .map(|d| d.with_timezone(&Utc))
        .ok()
}

/// Unknown dates pass: better a stale item the editor can reject than a
/// silent hole in the wire.
fn within_days(date: Option<&str>, days: i64) -> bool {
    match date.and_then(parse_date) {
        Some(d) => Utc::now().signed_duration_since(d).num_days() <= days,
        None => true,
    }
}

/// Strip tags, decode the handful of entities feeds actually use, squeeze
/// whitespace.
pub fn clean_text(s: &str) -> String {
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static WS: OnceLock<Regex> = OnceLock::new();
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]*>").unwrap());
    let ws = WS.get_or_init(|| Regex::new(r"\s+").unwrap());
    let no_tags = tags.replace_all(s, " ");
    let decoded = no_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    ws.replace_all(decoded.trim(), " ").to_string()
}

pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

async fn get_text(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|e| short_err(&e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }
    resp.text().await.map_err(|e| short_err(&e))
}

fn short_err(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "timed out".into()
    } else if e.is_connect() {
        "couldn't connect".into()
    } else {
        truncate(&e.to_string(), 120)
    }
}

// ---------------------------------------------------------------- og:image

/// Fetch the first ~400 KB of a page and pull out its share image.
pub async fn og_image(client: &reqwest::Client, page_url: &str) -> Option<String> {
    let fut = async {
        let mut resp = client
            .get(page_url)
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let final_url = resp.url().clone();
        let mut buf: Vec<u8> = Vec::new();
        while let Ok(Some(chunk)) = resp.chunk().await {
            buf.extend_from_slice(&chunk);
            if buf.len() > 400_000 {
                break;
            }
        }
        let html = String::from_utf8_lossy(&buf);
        let raw = og_image_from_html(&html)?;
        final_url.join(&raw).ok().map(|u| u.to_string())
    };
    tokio::time::timeout(Duration::from_secs(7), fut).await.ok().flatten()
}

pub fn og_image_from_html(html: &str) -> Option<String> {
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        [
            r#"(?is)<meta[^>]+(?:property|name)\s*=\s*["'](?:og:image(?::secure_url)?|twitter:image(?::src)?)["'][^>]*?content\s*=\s*["']([^"']+)["']"#,
            r#"(?is)<meta[^>]+content\s*=\s*["']([^"']+)["'][^>]*?(?:property|name)\s*=\s*["'](?:og:image(?::secure_url)?|twitter:image(?::src)?)["']"#,
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    });
    res.iter()
        .find_map(|re| re.captures(html).map(|c| c[1].trim().replace("&amp;", "&")))
        .filter(|u| !u.is_empty())
}

// -------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    const YT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <link rel="self" href="http://www.youtube.com/feeds/videos.xml?channel_id=UCzQUP1qoWDoEbmsQxvdjxgQ"/>
 <title>PowerfulJRE</title>
 <entry>
  <id>yt:video:abcdefghijk</id>
  <yt:videoId>abcdefghijk</yt:videoId>
  <yt:channelId>UCzQUP1qoWDoEbmsQxvdjxgQ</yt:channelId>
  <title>Joe Rogan Experience #9999 - Somebody &amp; Friends</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=abcdefghijk"/>
  <author><name>PowerfulJRE</name><uri>https://www.youtube.com/channel/UCzQUP1qoWDoEbmsQxvdjxgQ</uri></author>
  <published>2026-09-17T17:00:12+00:00</published>
  <updated>2026-09-17T19:00:00+00:00</updated>
  <media:group>
   <media:title>Joe Rogan Experience #9999</media:title>
   <media:content url="https://www.youtube.com/v/abcdefghijk?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i2.ytimg.com/vi/abcdefghijk/hqdefault.jpg" width="480" height="360"/>
   <media:description>A long talk.
With line breaks.</media:description>
  </media:group>
 </entry>
</feed>"#;

    const BING: &str = r#"<?xml version="1.0" encoding="utf-8" ?>
<rss version="2.0" xmlns:News="https://www.bing.com/news/search?q=x&amp;format=rss"><channel><title>x - BingNews</title>
<item><title>Starship flies again</title>
<link>http://www.bing.com/news/apiclick.aspx?ref=FexRss&amp;aid=&amp;tid=abc&amp;url=https%3a%2f%2fwww.example.com%2fspace%2fstarship%3fa%3d1&amp;c=123&amp;mkt=en-us</link>
<description>The &lt;b&gt;rocket&lt;/b&gt; went up &amp;amp; came down.</description>
<pubDate>Thu, 17 Sep 2026 14:05:00 GMT</pubDate>
<News:Source>Example News</News:Source>
<News:Image>http://www.bing.com/th?id=OVFT.abc&amp;pid=News</News:Image>
</item></channel></rss>"#;

    const GOOGLE: &str = r##"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/"><channel><title>q - Google News</title>
<item><title>Dodgers clinch the West - Los Angeles Times</title>
<link>https://news.google.com/rss/articles/CBMiabc?oc=5</link>
<guid isPermaLink="false">CBMiabc</guid>
<pubDate>Wed, 16 Sep 2026 05:12:00 GMT</pubDate>
<description>&lt;a href="https://news.google.com/rss/articles/CBMiabc"&gt;Dodgers clinch the West&lt;/a&gt;&amp;nbsp;&amp;nbsp;&lt;font color="#6f6f6f"&gt;Los Angeles Times&lt;/font&gt;</description>
<source url="https://www.latimes.com">Los Angeles Times</source></item></channel></rss>"##;

    #[test]
    fn parses_youtube_atom() {
        let e = parse_feed(YT);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].video_id.as_deref(), Some("abcdefghijk"));
        assert_eq!(e[0].title, "Joe Rogan Experience #9999 - Somebody & Friends");
        assert_eq!(e[0].link, "https://www.youtube.com/watch?v=abcdefghijk");
        assert_eq!(e[0].author.as_deref(), Some("PowerfulJRE"));
        assert_eq!(e[0].published.as_deref(), Some("2026-09-17T17:00:12Z"));
        assert_eq!(e[0].summary.as_deref(), Some("A long talk. With line breaks."));
        assert!(e[0].image.as_deref().unwrap().contains("hqdefault"));
    }

    #[test]
    fn parses_bing_rss_and_unwraps_link() {
        let e = parse_feed(BING);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].source.as_deref(), Some("Example News"));
        assert_eq!(e[0].summary.as_deref(), Some("The rocket went up & came down."));
        assert_eq!(unwrap_redirect(&e[0].link), "https://www.example.com/space/starship?a=1");
        assert!(e[0].image.is_some());
        assert_eq!(e[0].published.as_deref(), Some("2026-09-17T14:05:00Z"));
    }

    #[test]
    fn parses_google_news_rss() {
        let e = parse_feed(GOOGLE);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].source.as_deref(), Some("Los Angeles Times"));
        assert!(e[0].link.starts_with("https://news.google.com/"));
    }

    #[test]
    fn youtube_ids() {
        assert_eq!(youtube_id("https://www.youtube.com/watch?v=abcdefghijk&t=3s").as_deref(), Some("abcdefghijk"));
        assert_eq!(youtube_id("https://www.youtube.com/watch?app=desktop&v=abcdefghijk").as_deref(), Some("abcdefghijk"));
        assert_eq!(youtube_id("https://youtu.be/abcdefghijk?si=x").as_deref(), Some("abcdefghijk"));
        assert_eq!(youtube_id("https://www.youtube.com/shorts/abcdefghijk").as_deref(), Some("abcdefghijk"));
        assert_eq!(youtube_id("https://example.com/watch?v=abcdefghijk"), None);
    }

    #[test]
    fn channel_id_extraction() {
        let html = r#"<link rel="canonical" href="https://www.youtube.com/channel/UCzQUP1qoWDoEbmsQxvdjxgQ"><script>{"externalId":"UCzQUP1qoWDoEbmsQxvdjxgQ"}</script>"#;
        assert_eq!(channel_id_from_html(html).as_deref(), Some("UCzQUP1qoWDoEbmsQxvdjxgQ"));
        assert_eq!(channel_id_from_html("<html>nothing</html>"), None);
    }

    #[test]
    fn og_image_both_attribute_orders() {
        let a = r#"<meta property="og:image" content="https://x.test/a.jpg?w=1&amp;h=2">"#;
        let b = r#"<meta content='https://x.test/b.jpg' name='twitter:image'/>"#;
        assert_eq!(og_image_from_html(a).as_deref(), Some("https://x.test/a.jpg?w=1&h=2"));
        assert_eq!(og_image_from_html(b).as_deref(), Some("https://x.test/b.jpg"));
        assert_eq!(og_image_from_html("<meta name='description' content='x'>"), None);
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("héllo wörld", 6), "héllo…");
        assert_eq!(truncate("short", 10), "short");
    }
}
