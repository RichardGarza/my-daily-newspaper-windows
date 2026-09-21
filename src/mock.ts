// Sample data for running the front page in a plain browser (`npm run dev`)
// without the Tauri backend. Everything here is placeholder copy.

import type { Edition, InterestsFile, ScoreBugData, Status } from "./types";

const ph = (label: string, w = 960, h = 540) =>
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns='http://www.w3.org/2000/svg' width='${w}' height='${h}' viewBox='0 0 ${w} ${h}'>
      <defs><pattern id='d' width='6' height='6' patternUnits='userSpaceOnUse'><circle cx='3' cy='3' r='1.3' fill='#3a3a3a'/></pattern></defs>
      <rect width='100%' height='100%' fill='#bdb8ab'/><rect width='100%' height='100%' fill='url(#d)' opacity='.55'/>
      <text x='50%' y='52%' text-anchor='middle' font-family='Georgia,serif' font-size='${Math.round(h / 9)}' fill='#121212'>${label}</text>
    </svg>`
  );

// Placeholder body copy so the text-forward layout can be judged in a browser.
const SENTENCES = [
  "Placeholder copy stands in for the story the editor writes, so the columns can be judged with real weight on the page.",
  "It opens with the news, then gives the numbers, the names and the dates a reader would otherwise have to click through for.",
  "A second thought follows at a comfortable length, the way a wire writer files a short piece before deadline.",
  "Details arrive in the order they matter, and the paragraph ends before it overstays its welcome.",
  "What happens next gets a sentence of its own, because that is usually the part people actually want to know.",
  "None of this is real reporting; it is sample text shown only outside the desktop app.",
];
const body = (paras: number, perPara: number) =>
  Array.from({ length: paras }, (_, p) =>
    Array.from({ length: perPara }, (_, i) => SENTENCES[(p * 2 + i) % SENTENCES.length]).join(" ")
  ).join("\n\n");

const hoursAgo = (h: number) => new Date(Date.now() - h * 3600_000).toISOString();

export const mockEdition: Edition = {
  date: new Date().toLocaleDateString("en-CA"),
  generatedAt: new Date().toISOString(),
  editionNo: 12,
  tagline: "All the news that fits the beats",
  sections: ["Front Page", "Video Desk", "The X Wire", "Tech & AI", "Music", "Sports", "Range & Gear"],
  notes: ["Sample edition - this is placeholder copy shown outside the desktop app."],
  stats: { durationSecs: 94, searches: 9, pagesRead: 3, wireItems: 61, xPostsFromGrok: 12, costUsd: null },
  cards: [
    { id: "c0", kind: "video", size: "lead", section: "Front Page", headline: "Sample lead: a three-hour interview lands, and the rocket part starts at minute forty", dek: "Placeholder summary for the lead story. Two sentences that say what the item is and why it earned the top of the page today.", story: body(4, 3), source: "Sample Podcast", url: "https://example.com/lead", articleUrl: "https://example.com/lead-recap", image: ph("LEAD PHOTO"), published: hoursAgo(5), why: "Long-form, new today" },
    { id: "c1", kind: "article", size: "feature", section: "Front Page", headline: "Sample feature: new model ships with a longer memory and a shorter price list", dek: "Placeholder dek describing the release in plain words, without the hype.", story: body(3, 2), source: "Sample Tech Review", url: "https://example.com/f1", image: ph("PHOTO"), published: hoursAgo(9) },
    { id: "c2", kind: "article", size: "feature", section: "Front Page", headline: "Sample feature: surprise single arrives at midnight, album date follows", dek: "Placeholder dek. What dropped, where to hear it, and what it signals.", story: body(3, 2), source: "Sample Music Desk", url: "https://example.com/f2", videoUrl: "https://example.com/f2-video", published: hoursAgo(14) },
    { id: "c3", kind: "article", size: "feature", section: "Front Page", headline: "Sample feature: software update widens the rollout to older hardware", dek: "Placeholder dek noting which cars get it and what changes on the road.", story: body(3, 2), source: "Sample Auto Wire", url: "https://example.com/f3", image: ph("PHOTO"), published: hoursAgo(20), why: "Covers 2018 hardware" },
    { id: "c4", kind: "article", size: "feature", section: "Front Page", headline: "Sample feature: the home team clinches with a week to spare", dek: "Placeholder dek with the score, the standings and who is hurt.", story: body(3, 2), source: "Sample Sports Page", url: "https://example.com/f4", published: hoursAgo(11) },
    { id: "c5", kind: "article", size: "brief", section: "Front Page", headline: "Sample brief: battery maker claims a 20 percent density gain", dek: "One-line placeholder.", story: body(1, 4), source: "Sample Wire", url: "https://example.com/b1", published: hoursAgo(26) },
    { id: "c6", kind: "article", size: "brief", section: "Front Page", headline: "Sample brief: a humanoid robot folds laundry, slowly", dek: "One-line placeholder.", story: body(1, 4), source: "Sample Wire", url: "https://example.com/b2", published: hoursAgo(30) },
    { id: "c7", kind: "article", size: "brief", section: "Front Page", headline: "Sample brief: drone rules loosen for backyard flyers", dek: "One-line placeholder.", story: body(1, 4), source: "Sample Wire", url: "https://example.com/b3", published: hoursAgo(40) },

    { id: "c8", kind: "video", size: "feature", section: "Video Desk", headline: "Sample video: building an agent that files its own bug reports", dek: "Placeholder dek for a hands-on tutorial. What you will be able to build after watching.", story: body(3, 2), source: "Sample Channel", url: "https://example.com/v1", image: ph("VIDEO"), published: hoursAgo(7) },
    { id: "c9", kind: "video", size: "brief", section: "Video Desk", headline: "Sample video: episode 9999 with a guest who brought props", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Podcast", url: "https://example.com/v2", image: ph("VIDEO"), published: hoursAgo(22) },
    { id: "c10", kind: "video", size: "brief", section: "Video Desk", headline: "Sample video: the week in code, in six minutes", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Channel", url: "https://example.com/v3", image: ph("VIDEO"), published: hoursAgo(28) },
    { id: "c11", kind: "video", size: "brief", section: "Video Desk", headline: "Sample video: slide guitar tone on a budget", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Guitar", url: "https://example.com/v4", image: ph("VIDEO"), published: hoursAgo(50) },

    { id: "c12", kind: "post", size: "brief", section: "The X Wire", headline: "Sample post", dek: "Placeholder post text. Short, sharp, and exactly the sort of thing that would have been forwarded to you anyway.", story: body(1, 2), source: "X", author: "Sample Account (@sample_account)", url: "https://example.com/x1", published: hoursAgo(2) },
    { id: "c13", kind: "post", size: "brief", section: "The X Wire", headline: "Sample post", dek: "Another placeholder post. This one announces a launch window and includes a number.", story: body(1, 2), source: "X", author: "Sample Rockets (@sample_rockets)", url: "https://example.com/x2", published: hoursAgo(3) },
    { id: "c14", kind: "post", size: "brief", section: "The X Wire", headline: "Sample post", dek: "A third placeholder post, slightly longer than the others so the columns have something uneven to balance against when the section is laid out.", story: body(1, 2), source: "X", author: "Sample Dev (@sample_dev)", url: "https://example.com/x3", published: hoursAgo(6) },
    { id: "c15", kind: "post", size: "brief", section: "The X Wire", headline: "Sample post", dek: "Fourth placeholder. Dry joke goes here.", story: body(1, 2), source: "X", author: "Sample Humor (@sample_humor)", url: "https://example.com/x4", published: hoursAgo(8) },

    { id: "c16", kind: "article", size: "feature", section: "Tech & AI", headline: "Sample: a command-line agent learns to drive a browser", dek: "Placeholder dek explaining the feature and the one caveat that matters.", story: body(3, 2), source: "Sample Dev Blog", url: "https://example.com/t1", image: ph("PHOTO"), published: hoursAgo(16) },
    { id: "c17", kind: "article", size: "brief", section: "Tech & AI", headline: "Sample: open-weights model runs on a laptop", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Wire", url: "https://example.com/t2", published: hoursAgo(19) },
    { id: "c18", kind: "article", size: "brief", section: "Tech & AI", headline: "Sample: game engine update halves build times", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Wire", url: "https://example.com/t3", published: hoursAgo(33) },

    { id: "c19", kind: "article", size: "brief", section: "Music", headline: "Sample: guitarist announces a winter theater run", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Music Desk", url: "https://example.com/m1", published: hoursAgo(27) },
    { id: "c20", kind: "video", size: "brief", section: "Music", headline: "Sample: official video for the new single", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Artist", url: "https://example.com/m2", image: ph("VIDEO"), published: hoursAgo(13) },

    { id: "c21", kind: "article", size: "brief", section: "Sports", headline: "Sample: three waiver pickups before Sunday", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Fantasy", url: "https://example.com/s1", published: hoursAgo(10) },
    { id: "c22", kind: "article", size: "brief", section: "Sports", headline: "Sample: start him, sit him - week three", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Fantasy", url: "https://example.com/s2", published: hoursAgo(12) },

    { id: "c23", kind: "article", size: "brief", section: "Range & Gear", headline: "Sample: a compact optic gets a bigger window", dek: "Placeholder dek.", story: body(1, 4), source: "Sample Gear Review", url: "https://example.com/g1", published: hoursAgo(44) },
  ],
};

export const mockInterests: InterestsFile = {
  version: 1,
  interests: [
    { id: "a", name: "Elon Musk", kind: "person", value: "Elon Musk interview OR Tesla OR SpaceX", notes: "New long-form interviews first.", enabled: true },
    { id: "b", name: "The Joe Rogan Experience", kind: "youtube_channel", value: "@joerogan", notes: "Newest full episodes.", enabled: true },
    { id: "c", name: "Drake", kind: "person", value: "", notes: "Latest drops only.", enabled: true },
    { id: "d", name: "AI instructionals", kind: "topic", value: "", notes: "Hands-on tutorials over hype.", enabled: true },
    { id: "e", name: "LA Dodgers", kind: "topic", value: "Los Angeles Dodgers", notes: "", enabled: false },
  ],
};

export const mockScript: Array<Omit<Status, "at"> & { wait: number }> = [
  { stage: "wire", message: "Warming up the presses", detail: "Finding Claude and Grok", wait: 500 },
  { stage: "wire", message: "Pulling the wires", detail: "YouTube channels and news feeds", wait: 700 },
  { stage: "grok", message: "Grok is searching X", detail: "Posts from the last 48 hours", wait: 1200 },
  { stage: "wire", message: "Wire copy is in", detail: "61 feed items · 12 X posts", wait: 600 },
  { stage: "research", message: "Claude's researching", detail: "new long-form interview this week", wait: 1100 },
  { stage: "research", message: "Claude's reading", detail: "example.com", wait: 900 },
  { stage: "research", message: "Claude's researching", detail: "new single released this week", wait: 1000 },
  { stage: "writing", message: "Building new Daily brief", detail: "Writing headlines", wait: 1500 },
  { stage: "content", message: "Loading web content", detail: "Thumbnails and lead images", wait: 900 },
];

export const mockScore = (live: boolean): ScoreBugData =>
  live
    ? {
        game: {
          gamePk: 2, state: "live", detail: "Bot 7th", start: new Date(Date.now() - 2 * 3600_000).toISOString(),
          away: { abbr: "AWY", name: "Visitors", score: 2 }, home: { abbr: "LAD", name: "Dodgers", score: 4 },
          weAreHome: true, url: "https://example.com/gameday",
          tv: ["SportsNet LA"],
          situation: {
            balls: 2, strikes: 1, outs: 1, bases: [true, false, true],
            batter: "F. Freeman", pitcher: "A. Reliever", challenges: [1, 2],
          },
        },
        next: null,
      }
    : {
        game: {
          gamePk: 1, state: "final", detail: "Final", start: new Date(Date.now() - 20 * 3600_000).toISOString(),
          away: { abbr: "AWY", name: "Visitors", score: 3 }, home: { abbr: "LAD", name: "Dodgers", score: 5 },
          weAreHome: true, url: "https://example.com/gameday",
        },
        next: {
          gamePk: 3, state: "preview", detail: "Scheduled", start: new Date(Date.now() + 5 * 3600_000).toISOString(),
          away: { abbr: "LAD", name: "Dodgers", score: null }, home: { abbr: "HME", name: "Hosts", score: null },
          weAreHome: false, url: "https://example.com/gameday-next",
          tv: ["SportsNet LA", "FOX"],
        },
      };
