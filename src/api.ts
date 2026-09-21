// Thin wrapper over the Tauri backend. Outside the desktop app (plain
// `npm run dev` in a browser) it falls back to sample data so the front page
// can still be worked on.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Diagnostics, Edition, InterestsFile, PrintStatus, Profile, ScheduleInfo, ScoreBugData, Status } from "./types";
import { mockEdition, mockInterests, mockScore, mockScript } from "./mock";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const mockListeners = new Set<(s: Status) => void>();
let mockInterestsState: InterestsFile = structuredClone(mockInterests);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function loadEdition(): Promise<Edition | null> {
  if (inTauri) return invoke<Edition | null>("load_edition");
  const params = new URLSearchParams(window.location.search);
  if (params.has("empty")) return null;
  // ?solo=1 / ?duo=1: front page with no (or one) feature beside the lead.
  if (params.has("solo") || params.has("duo")) {
    let kept = params.has("duo") ? 1 : 0;
    return {
      ...mockEdition,
      cards: mockEdition.cards.map((c) =>
        c.section === "Front Page" && c.size === "feature" && kept-- <= 0 ? { ...c, section: "Tech & AI" } : c
      ),
    };
  }
  return mockEdition;
}

export async function today(): Promise<string> {
  if (inTauri) return invoke<string>("today");
  return new Date().toLocaleDateString("en-CA");
}

export async function refreshEdition(): Promise<Edition> {
  if (inTauri) return invoke<Edition>("refresh_edition");
  for (const step of mockScript) {
    mockListeners.forEach((fn) => fn({ ...step, at: Date.now() }));
    await sleep(step.wait);
  }
  const fresh = { ...mockEdition, generatedAt: new Date().toISOString() };
  mockListeners.forEach((fn) =>
    fn({ stage: "done", message: "Edition ready", detail: `${fresh.cards.length} stories · 9s`, at: Date.now() })
  );
  return fresh;
}

export async function onStatus(fn: (s: Status) => void): Promise<() => void> {
  if (inTauri) return listen<Status>("daily-status", (e) => fn(e.payload));
  mockListeners.add(fn);
  return () => mockListeners.delete(fn);
}

export async function getInterests(): Promise<InterestsFile> {
  if (inTauri) return invoke<InterestsFile>("get_interests");
  return structuredClone(mockInterestsState);
}

export async function saveInterests(file: InterestsFile): Promise<InterestsFile> {
  if (inTauri) return invoke<InterestsFile>("save_interests", { file });
  mockInterestsState = structuredClone(file);
  return file;
}

export async function resetInterests(): Promise<InterestsFile> {
  if (inTauri) return invoke<InterestsFile>("reset_interests");
  mockInterestsState = structuredClone(mockInterests);
  return structuredClone(mockInterestsState);
}

export async function diagnostics(): Promise<Diagnostics | null> {
  if (inTauri) return invoke<Diagnostics>("diagnostics");
  return { claudePath: "/usr/local/bin/claude", grokPath: null, dataDir: "(browser preview)" };
}

export async function openReader(url: string, title: string): Promise<void> {
  if (inTauri) return invoke<void>("open_reader", { url, title });
  window.open(url, "_blank", "popup,width=1100,height=760");
}

export async function scoreBug(): Promise<ScoreBugData> {
  if (inTauri) return invoke<ScoreBugData>("score_bug");
  const params = new URLSearchParams(window.location.search);
  return mockScore(params.has("live"));
}

let mockSchedule: ScheduleInfo = { supported: true, enabled: false, hour: 6, minute: 0 };

export async function getSchedule(): Promise<ScheduleInfo> {
  if (inTauri) return invoke<ScheduleInfo>("get_schedule");
  return mockSchedule;
}

export async function setSchedule(enabled: boolean, hour: number, minute = 0): Promise<ScheduleInfo> {
  if (inTauri) return invoke<ScheduleInfo>("set_schedule", { enabled, hour, minute });
  mockSchedule = { supported: true, enabled, hour, minute };
  return mockSchedule;
}

// ------------------------------------------------------------ whose paper

const params = () => new URLSearchParams(window.location.search);
let mockProfile: Profile = {
  ownerName: "Sam",
  paperName: "Sam\u2019s Daily",
  city: "Los Angeles",
  mlbTeamId: 119,
  onboarded: true,
};

export async function getProfile(): Promise<Profile> {
  if (inTauri) return invoke<Profile>("get_profile");
  // ?welcome=1 previews the first-run pages in a browser
  if (params().has("welcome") && mockProfile.onboarded && !sessionStorage.getItem("rd-welcomed")) {
    mockProfile = { ...mockProfile, ownerName: "", paperName: "My Daily", onboarded: false };
  }
  return mockProfile;
}

export async function setProfile(p: { ownerName: string; city: string; mlbTeamId: number; onboarded: boolean }): Promise<Profile> {
  if (inTauri) return invoke<Profile>("set_profile", p);
  const name = p.ownerName.trim();
  mockProfile = { ...p, ownerName: name, paperName: name ? `${name}\u2019s Daily` : "My Daily" };
  if (p.onboarded) sessionStorage.setItem("rd-welcomed", "1");
  return mockProfile;
}

// ---------------------------------------------------------------- printing

let mockPrint: PrintStatus = { printDaily: false, browserFound: true, printer: "Sample_LaserJet", problem: null };

export async function printStatus(): Promise<PrintStatus> {
  if (inTauri) return invoke<PrintStatus>("print_status");
  return mockPrint;
}

export async function setPrintDaily(enabled: boolean): Promise<PrintStatus> {
  if (inTauri) return invoke<PrintStatus>("set_print_daily", { enabled });
  mockPrint = { ...mockPrint, printDaily: enabled };
  return mockPrint;
}

/** "preview" opens the PDF; "printer" sends it straight to the printer. */
export async function printEdition(target: "preview" | "printer"): Promise<string> {
  if (inTauri) return invoke<string>("print_edition", { target });
  await sleep(900);
  return "(browser preview: no PDF made)";
}
