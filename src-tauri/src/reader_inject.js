// Injected into every page the reader window loads.
// Adds a thin newspaper frame plus Close / Full Screen controls, and blocks ads
// (config arrives as window.__rdAdblock, written by adblock.rs).
//
// Rules this script lives by, because it runs inside other people's pages:
//  - top frame only
//  - no innerHTML and no <style> tags (strict CSP / Trusted Types sites block
//    them); every style is set through the CSSOM, which CSP allows
//  - no access to the app: buttons "navigate" to a sentinel URL that the Rust
//    side intercepts and cancels
(function () {
  if (window.top !== window) return;
  if (window.__myDailyReader) return;
  window.__myDailyReader = true;

  var SENTINEL = "https://reader.my-daily.invalid/";

  // ------------------------------------------------------------ ad blocking
  // Layer 2 of 2 (layer 1 is the navigation handler in reader.rs, which
  // cancels ad iframes). Here: ad scripts are neutered before they run, and
  // the usual ad containers are hidden. Never active on YouTube or X.
  var AB = window.__rdAdblock || { on: false, hosts: [], exempt: [] };
  var blockedCount = 0;
  var adLabel = null;

  function hostMatches(host, list) {
    host = String(host || "").toLowerCase().replace(/\.$/, "");
    for (var i = 0; i < list.length; i++) {
      var d = list[i];
      if (host === d || host.slice(-(d.length + 1)) === "." + d) return true;
    }
    return false;
  }

  function isAdUrl(u) {
    if (!u) return false;
    try {
      var parsed = new URL(String(u), location.href);
      if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;
      return hostMatches(parsed.hostname, AB.hosts);
    } catch (e) {
      return false;
    }
  }

  var adsAllowedHere = false;
  try {
    adsAllowedHere = window.sessionStorage.getItem("__rdAdsOff") === "1";
  } catch (e) {}
  var exemptPage = hostMatches(location.hostname, AB.exempt || []);
  var adblockActive = !!AB.on && !exemptPage && !adsAllowedHere;

  function noteBlocked(n) {
    blockedCount += n || 1;
    if (adLabel) adLabel.textContent = adLabelText();
  }

  function adLabelText() {
    if (!adblockActive) return "Ad block off";
    return blockedCount > 0 ? "Ad block on \u00b7 " + blockedCount : "Ad block on";
  }

  var AD_SELECTORS = [
    "ins.adsbygoogle",
    "iframe[id^='google_ads_iframe']",
    "div[id^='google_ads_iframe']",
    "div[id^='div-gpt-ad']",
    "[data-google-query-id]",
    "[data-ad-slot]",
    "[data-ad-unit]",
    "[id^='taboola-']",
    ".trc_related_container",
    ".OUTBRAIN",
    "[data-widget-id^='AR_']",
    "[aria-label='Advertisement']",
    "[aria-label='advertisement']",
    ".ad-container",
    ".ad-wrapper",
    ".ad-slot",
    ".ad-unit",
    ".advertisement",
    ".cnx-main-container",
  ];

  if (adblockActive) {
    // (a) Scripts written into the HTML: the parser adds the element, then
    // gives observers a turn before it prepares the script, so retyping it
    // here stops it from ever running.
    var neuter = function (node) {
      if (!node || node.nodeType !== 1) return;
      var tag = node.tagName;
      if (tag === "SCRIPT") {
        var src = node.getAttribute("src");
        if (src && node.type !== "javascript/blocked" && isAdUrl(src)) {
          node.type = "javascript/blocked";
          if (node.parentNode) node.parentNode.removeChild(node);
          noteBlocked();
        }
      } else if (tag === "IFRAME" || tag === "IMG") {
        if (isAdUrl(node.getAttribute("src"))) {
          if (node.parentNode) node.parentNode.removeChild(node);
          noteBlocked();
        }
      }
    };
    try {
      new MutationObserver(function (muts) {
        for (var i = 0; i < muts.length; i++) {
          var added = muts[i].addedNodes;
          for (var j = 0; j < added.length; j++) {
            var n = added[j];
            if (n.nodeType !== 1) continue;
            neuter(n);
            if (n.firstElementChild && n.querySelectorAll) {
              var inner = n.querySelectorAll("script[src],iframe[src]");
              for (var k = 0; k < inner.length; k++) neuter(inner[k]);
            }
          }
        }
      }).observe(document, { childList: true, subtree: true });
    } catch (e) {}

    // (b) Scripts created by other scripts: catch the src as it is assigned.
    try {
      var srcDesc = Object.getOwnPropertyDescriptor(HTMLScriptElement.prototype, "src");
      var realCreate = Document.prototype.createElement;
      Document.prototype.createElement = function () {
        var el = realCreate.apply(this, arguments);
        try {
          if (el && el.tagName === "SCRIPT" && srcDesc && srcDesc.set) {
            Object.defineProperty(el, "src", {
              configurable: true,
              enumerable: true,
              get: function () {
                return srcDesc.get.call(this);
              },
              set: function (v) {
                if (isAdUrl(v)) {
                  this.type = "javascript/blocked";
                  noteBlocked();
                }
                srcDesc.set.call(this, v);
              },
            });
            var realSetAttribute = el.setAttribute;
            el.setAttribute = function (name, value) {
              if (String(name).toLowerCase() === "src" && isAdUrl(value)) {
                realSetAttribute.call(this, "type", "javascript/blocked");
                noteBlocked();
              }
              return realSetAttribute.apply(this, arguments);
            };
          }
        } catch (e) {}
        return el;
      };
    } catch (e) {}

    // (c) Hide what is left. A constructed stylesheet is CSSOM, so strict
    // site CSPs allow it; the sweep below covers engines without it.
    var hideRule = AD_SELECTORS.join(",") + "{display:none!important;visibility:hidden!important}";
    try {
      var sheet = new CSSStyleSheet();
      sheet.replaceSync(hideRule);
      document.adoptedStyleSheets = [].concat(Array.prototype.slice.call(document.adoptedStyleSheets || []), [sheet]);
    } catch (e) {}
    var sweeps = 0;
    var sweep = function () {
      try {
        var found = document.querySelectorAll(AD_SELECTORS.join(","));
        var fresh = 0;
        for (var i = 0; i < found.length; i++) {
          var el = found[i];
          if (el.__rdHidden) continue;
          el.__rdHidden = true;
          el.style.setProperty("display", "none", "important");
          fresh++;
        }
        if (fresh) noteBlocked(fresh);
      } catch (e) {}
      if (++sweeps < 20) setTimeout(sweep, sweeps < 6 ? 700 : 2500);
    };
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", sweep);
    else sweep();
  }

  function toggleAds() {
    try {
      if (adblockActive) window.sessionStorage.setItem("__rdAdsOff", "1");
      else window.sessionStorage.removeItem("__rdAdsOff");
    } catch (e) {}
    send(adblockActive ? "ads-off" : "ads-on");
    setTimeout(function () {
      try {
        window.location.reload();
      } catch (e) {}
    }, 250);
  }

  // ------------------------------------------------------------ window chrome
  var INK = "#121212";
  var PAPER = "#f7f4ec";

  function send(action) {
    try {
      window.location.href = SENTINEL + action;
    } catch (e) {}
  }

  function css(el, styles) {
    for (var k in styles) el.style.setProperty(k, styles[k], "important");
    return el;
  }

  function button(label, title, action) {
    var b = document.createElement("button");
    b.type = "button";
    b.textContent = label;
    b.title = title;
    css(b, {
      all: "initial",
      font: "600 11px/1 Georgia, 'Times New Roman', serif",
      "letter-spacing": "0.12em",
      "text-transform": "uppercase",
      color: PAPER,
      background: INK,
      border: "1px solid " + PAPER,
      padding: "7px 10px",
      cursor: "pointer",
      "user-select": "none",
      "-webkit-user-select": "none",
    });
    b.addEventListener("mouseenter", function () {
      css(b, { background: PAPER, color: INK, "border-color": INK });
    });
    b.addEventListener("mouseleave", function () {
      css(b, { background: INK, color: PAPER, "border-color": PAPER });
    });
    b.addEventListener("click", function (e) {
      e.preventDefault();
      e.stopPropagation();
      if (action) send(action);
    });
    return b;
  }

  var host = null;

  function build() {
    host = document.createElement("div");
    host.setAttribute("data-my-daily", "reader-chrome");
    css(host, {
      all: "initial",
      position: "fixed",
      inset: "0",
      "z-index": "2147483647",
      "pointer-events": "none",
    });

    var root = host.attachShadow ? host.attachShadow({ mode: "closed" }) : host;

    // The border: a double rule, like a boxed story.
    var frame = document.createElement("div");
    css(frame, {
      position: "absolute",
      inset: "0",
      border: "3px solid " + INK,
      "box-shadow": "inset 0 0 0 2px " + PAPER + ", inset 0 0 0 3px " + INK,
      "pointer-events": "none",
      "box-sizing": "border-box",
    });

    var bar = document.createElement("div");
    css(bar, {
      position: "absolute",
      top: "12px",
      right: "14px",
      display: "flex",
      gap: "6px",
      "pointer-events": "auto",
      opacity: "0.65",
      transition: "opacity 120ms linear",
    });
    bar.addEventListener("mouseenter", function () {
      css(bar, { opacity: "1" });
    });
    bar.addEventListener("mouseleave", function () {
      css(bar, { opacity: "0.65" });
    });

    if (AB.on && !exemptPage) {
      adLabel = button(adLabelText(), "Ad blocking for this window. Click to switch it off (or back on) and reload the page - handy when a site refuses to load with a blocker.", null);
      adLabel.addEventListener("click", function () {
        toggleAds();
      });
      bar.appendChild(adLabel);
    }
    bar.appendChild(button("Full screen", "Toggle full screen", "fullscreen"));
    bar.appendChild(button("✕ Close", "Close (Esc)", "close"));

    root.appendChild(frame);
    root.appendChild(bar);
  }

  function attach() {
    if (!document.documentElement) return;
    if (!host) build();
    if (host.parentNode !== document.documentElement) {
      document.documentElement.appendChild(host);
    }
  }

  // Esc closes - unless you're typing, or a video is in its own full screen.
  window.addEventListener(
    "keydown",
    function (e) {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (document.fullscreenElement || document.webkitFullscreenElement) return;
      var a = document.activeElement;
      var tag = a && a.tagName ? a.tagName.toLowerCase() : "";
      if (tag === "input" || tag === "textarea" || tag === "select" || (a && a.isContentEditable)) return;
      send("close");
    },
    false
  );

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", attach);
  } else {
    attach();
  }
  window.addEventListener("load", attach);
  // Single-page apps sometimes rebuild the document; put the chrome back.
  setInterval(attach, 1500);
})();
