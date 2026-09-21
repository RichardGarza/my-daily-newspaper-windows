//! Ad blocking for the reader windows.
//!
//! Two layers, one list:
//!   1. Rust (`reader.rs`): every frame navigation passes through the reader's
//!      navigation handler, so ad iframes and click-through redirects to
//!      ad-tech hosts are cancelled before they load.
//!   2. JavaScript (`reader_inject.js`): ad scripts are neutered as the page
//!      is parsed, and the usual ad containers are hidden.
//!
//! The list is deliberately short and boring: big ad-tech domains only. No
//! analytics, no tag managers, no CDNs - blocking those breaks sites.
//!
//! YouTube and X are left completely alone. Their ads are first-party, and
//! YouTube in particular stops playing video when it thinks it is being
//! blocked. A reader that can't play the video is worse than a pre-roll.

use url::Url;

pub const AD_HOSTS: &[&str] = &[
    // Google ad stack
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "googletagservices.com",
    "adservice.google.com",
    "2mdn.net",
    // Amazon
    "amazon-adsystem.com",
    // exchanges, SSPs, DSPs
    "adnxs.com",
    "adsrvr.org",
    "rubiconproject.com",
    "pubmatic.com",
    "openx.net",
    "criteo.com",
    "criteo.net",
    "casalemedia.com",
    "indexww.com",
    "3lift.com",
    "triplelift.com",
    "sharethrough.com",
    "smartadserver.com",
    "teads.tv",
    "yieldmo.com",
    "media.net",
    "bidswitch.net",
    "adform.net",
    "advertising.com",
    "serving-sys.com",
    "mathtag.com",
    "gumgum.com",
    "sonobi.com",
    "lijit.com",
    "sovrn.com",
    "contextweb.com",
    "spotxchange.com",
    "spotx.tv",
    "springserve.com",
    "tremorhub.com",
    "adroll.com",
    "nativo.com",
    "adsymptotic.com",
    // "around the web" boxes
    "taboola.com",
    "outbrain.com",
    "zergnet.com",
    "revcontent.com",
    "mgid.com",
    // autoplay video ad units
    "connatix.com",
    "primis.tech",
    "vidazoo.com",
    "aniview.com",
    // ad management wrappers
    "adthrive.com",
    "mediavine.com",
    "pub.network",
    "htlbid.com",
    // ad verification / audience trackers that ride along with the ads
    "moatads.com",
    "adsafeprotected.com",
    "doubleverify.com",
    "scorecardresearch.com",
    "quantserve.com",
    "bluekai.com",
    "krxd.net",
    "rlcdn.com",
    "agkn.com",
    "liadm.com",
    "id5-sync.com",
];

/// Pages where the blocker switches itself off entirely.
const EXEMPT_PAGES: &[&str] = &["youtube.com", "youtu.be", "youtube-nocookie.com", "x.com", "twitter.com"];

fn matches(host: &str, list: &[&str]) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    list.iter().any(|d| h == *d || h.ends_with(&format!(".{d}")))
}

pub fn is_ad_host(host: &str) -> bool {
    matches(host, AD_HOSTS)
}

pub fn is_ad_url(u: &Url) -> bool {
    matches!(u.scheme(), "http" | "https") && u.host_str().map(is_ad_host).unwrap_or(false)
}

pub fn is_exempt_page(host: &str) -> bool {
    matches(host, EXEMPT_PAGES)
}

/// The config object the injected script reads.
pub fn script_config(enabled: bool) -> String {
    format!(
        "window.__rdAdblock = {{ on: {}, hosts: {}, exempt: {} }};\n",
        enabled,
        serde_json::to_string(AD_HOSTS).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string(EXEMPT_PAGES).unwrap_or_else(|_| "[]".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_domains_and_subdomains_only() {
        assert!(is_ad_host("doubleclick.net"));
        assert!(is_ad_host("securepubads.g.doubleclick.net"));
        assert!(is_ad_host("TPC.GoogleSyndication.com."));
        assert!(!is_ad_host("notdoubleclick.net"));
        assert!(!is_ad_host("doubleclick.net.evil.example"));
        assert!(!is_ad_host("www.nytimes.com"));
        assert!(!is_ad_host("www.googletagmanager.com"));
        assert!(!is_ad_host("i.ytimg.com"));
    }

    #[test]
    fn urls_and_exemptions() {
        assert!(is_ad_url(&Url::parse("https://googleads.g.doubleclick.net/pagead/ads?x=1").unwrap()));
        assert!(!is_ad_url(&Url::parse("https://www.theverge.com/ads-are-bad").unwrap()));
        assert!(!is_ad_url(&Url::parse("about:blank").unwrap()));
        assert!(is_exempt_page("www.youtube.com"));
        assert!(is_exempt_page("x.com"));
        assert!(!is_exempt_page("www.espn.com"));
    }

    #[test]
    fn config_is_valid_javascript_data() {
        let cfg = script_config(true);
        assert!(cfg.starts_with("window.__rdAdblock = { on: true, hosts: [\"doubleclick.net\""));
        assert!(cfg.trim_end().ends_with("};"));
    }
}
