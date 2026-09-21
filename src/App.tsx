import { useCallback, useEffect, useRef, useState } from "react";
import type { Diagnostics, Edition, PrintStatus, Profile, ScheduleInfo, Status } from "./types";
import { diagnostics, getProfile, getSchedule, loadEdition, onStatus, printEdition, printStatus, refreshEdition, today as fetchToday } from "./api";
import { longDate, roman, slug } from "./util";
import PressRoom from "./components/PressRoom";
import ScoreBug from "./components/ScoreBug";
import Delivery from "./components/Delivery";
import FrontPage from "./components/FrontPage";
import InterestsPage from "./components/InterestsPage";
import Welcome from "./components/Welcome";

type View = "front" | "interests" | "welcome";

export default function App() {
  const [view, setView] = useState<View>("front");
  const [edition, setEdition] = useState<Edition | null>(null);
  const [todayStr, setTodayStr] = useState<string>("");
  const [loaded, setLoaded] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [log, setLog] = useState<Status[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [schedule, setSchedule] = useState<ScheduleInfo | null>(null);
  const [profile, setProfileState] = useState<Profile | null>(null);
  const [print, setPrint] = useState<PrintStatus | null>(null);
  const [printing, setPrinting] = useState(false);
  const busy = useRef(false);
  const booted = useRef(false);

  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setRefreshing(true);
    setStartedAt(Date.now());
    setLog([]);
    setError(null);
    setNotice(null);
    try {
      const fresh = await refreshEdition();
      setEdition(fresh);
      window.scrollTo({ top: 0 });
    } catch (e) {
      const msg = String(e);
      if (msg.includes("in the background")) setNotice("An edition is printing in the background. It will appear here when it's done.");
      else if (!msg.includes("already being built")) setError(msg);
    } finally {
      busy.current = false;
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    if (booted.current) return;
    booted.current = true;

    let unlisten: (() => void) | undefined;
    onStatus((s) => setLog((prev) => [...prev.slice(-40), s])).then((fn) => (unlisten = fn));

    (async () => {
      const [t, e, who] = await Promise.all([fetchToday(), loadEdition().catch(() => null), getProfile().catch(() => null)]);
      setTodayStr(t);
      setEdition(e);
      setProfileState(who);
      setLoaded(true);
      diagnostics().then(setDiag).catch(() => {});
      getSchedule().then(setSchedule).catch(() => {});
      printStatus().then(setPrint).catch(() => {});
      // A brand-new subscriber meets the welcome pages before anything is
      // researched on their behalf; the first edition prints when they finish.
      if (who && !who.onboarded) {
        setView("welcome");
        return;
      }
      // A new day (or a first run) prints a new edition on its own.
      if (!e || e.date !== t) void refresh();
    })();

    return () => unlisten?.();
  }, [refresh]);

  // The scheduled background run is a separate copy of the app: it just leaves
  // a newer edition on disk. Check for one every few minutes and when the
  // window regains focus. If the app has been open since yesterday and nothing
  // has delivered today's paper by a sensible hour, print it ourselves.
  const touring = useRef(false);
  touring.current = !!profile && !profile.onboarded;
  const editionRef = useRef<Edition | null>(null);
  editionRef.current = edition;
  const scheduleRef = useRef<ScheduleInfo | null>(null);
  scheduleRef.current = schedule;
  useEffect(() => {
    const check = async () => {
      if (busy.current || touring.current) return;
      try {
        const [t, onDisk] = await Promise.all([fetchToday(), loadEdition()]);
        setTodayStr(t);
        const current = editionRef.current;
        if (onDisk && (!current || onDisk.generatedAt > current.generatedAt)) {
          setEdition(onDisk);
          setError(null);
          setNotice(null);
          return;
        }
        const haveToday = (onDisk ?? current)?.date === t;
        if (!haveToday) {
          const s = scheduleRef.current;
          const d = new Date();
          const minutesNow = d.getHours() * 60 + d.getMinutes();
          // give the background job a 20-minute head start; with delivery off, wait for 5 AM
          const threshold = s?.enabled ? s.hour * 60 + s.minute + 20 : 5 * 60;
          if (minutesNow >= threshold) void refresh();
        }
      } catch {
        /* try again next time */
      }
    };
    const timer = window.setInterval(check, 5 * 60 * 1000);
    const onFocus = () => void check();
    window.addEventListener("focus", onFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const paper = profile?.paperName ?? "My Daily";
  useEffect(() => {
    document.title = paper;
  }, [paper]);

  const makePdf = async () => {
    setPrinting(true);
    setNotice(null);
    try {
      await printEdition("preview");
      setNotice("The paper edition is open in your PDF viewer. Print it from there.");
    } catch (e) {
      setNotice(String(e));
    } finally {
      setPrinting(false);
    }
  };

  const now = new Date();
  const stale = !!edition && !!todayStr && edition.date !== todayStr;

  return (
    <div className="paper">
      <div className="utility">
        <div className="utility-left">
          {view === "front" ? (
            <button className="link-btn" onClick={() => setView("interests")} disabled={refreshing}>
              Edit Interests
            </button>
          ) : (
            <span className="utility-note">{view === "welcome" ? "Welcome" : "Editing interests"}</span>
          )}
        </div>
        <ScoreBug teamId={view === "welcome" ? 0 : (profile?.mlbTeamId ?? 0)} />
        <div className="utility-right">
          {diag && !diag.claudePath && <span className="utility-warn">Claude CLI not found</span>}
          {view === "front" && edition && (
            <button
              className="link-btn"
              onClick={() => void makePdf()}
              disabled={printing || refreshing}
              title={print?.problem ?? "Lay the edition out for paper and open the PDF"}
            >
              {printing ? "Laying out\u2026" : "Print"}
            </button>
          )}
          {view === "front" && (
            <button className="refresh" onClick={refresh} disabled={refreshing}>
              <span className={`refresh-glyph ${refreshing ? "spin" : ""}`} aria-hidden="true">
                ↻
              </span>
              {refreshing ? "Printing…" : "Refresh"}
            </button>
          )}
        </div>
      </div>

      <header className="masthead">
        <aside className="ear ear-left">
          <div className="ear-label">{edition && !stale ? "Today's Edition" : stale ? "Previous Edition" : "First Edition"}</div>
          <div className="ear-big">{longDate(now)}</div>
          <div className="press-detail">
            {stale && edition ? `Showing ${edition.date} while today\u2019s prints` : "Edited by Claude \u00b7 X wire by Grok"}
          </div>
          <Delivery schedule={schedule} onChange={setSchedule} print={print} onPrint={setPrint} />
        </aside>

        <h1
          className={`nameplate ${paper.length > 16 ? "long" : ""}`}
          onClick={() => view === "interests" && setView("front")}
          title={view === "interests" ? "Front page" : undefined}
        >
          {paper}
        </h1>

        <PressRoom refreshing={refreshing} log={log} startedAt={startedAt} edition={edition} error={error} />
      </header>

      <div className="dateline">
        <span>
          Vol. {roman(now.getFullYear() - 2025)} . . . No. {edition?.editionNo ?? 1}
        </span>
        <span className="dateline-center">{profile?.city ? `${profile.city}, ` : ""}{longDate(now)}</span>
        <span className="dateline-right">{edition?.tagline ? `“${edition.tagline}”` : "“All the news that fits the beats”"}</span>
      </div>

      {view === "front" && edition && edition.sections.length > 1 && (
        <nav className="sections-nav">
          {edition.sections.map((s) => (
            <a key={s} href={`#${slug(s)}`}>
              {s}
            </a>
          ))}
        </nav>
      )}

      {notice && view === "front" && (
        <div className="notice">
          {notice}{" "}
          {notice.startsWith("Saved") && (
            <button className="link-btn" onClick={refresh} disabled={refreshing}>
              Refresh now
            </button>
          )}
        </div>
      )}

      {view === "welcome" && profile ? (
        <Welcome
          profile={profile}
          onDone={(p) => {
            setProfileState(p);
            setView("interests");
            window.scrollTo({ top: 0 });
          }}
        />
      ) : view === "interests" && profile ? (
        <InterestsPage
          profile={profile}
          onProfile={setProfileState}
          firstRun={!profile.onboarded}
          onClose={(saved) => {
            const wasFirstRun = !profile.onboarded;
            setView("front");
            window.scrollTo({ top: 0 });
            if (wasFirstRun) void refresh();
            else if (saved) setNotice("Saved. Changes to your interests take effect on the next edition.");
          }}
        />
      ) : edition ? (
        <FrontPage edition={edition} />
      ) : (
        <main className="page">
          <div className="blank-page">
            {!loaded ? (
              <p>Unfolding the paper…</p>
            ) : refreshing ? (
              <>
                <h2>The presses are rolling.</h2>
                <p>
                  Your first edition is being researched, written and laid out. It usually takes two to four minutes. Progress is in the Press Room,
                  top right.
                </p>
              </>
            ) : (
              <>
                <h2>No edition on the stands.</h2>
                <p>{error ?? "Hit Refresh to print one."}</p>
              </>
            )}
          </div>
        </main>
      )}

      <footer className="colophon">
        <span>{paper} · printed on demand · one subscriber</span>
        {diag && <span title={diag.dataDir}>Editions and interests live in {diag.dataDir}</span>}
      </footer>
    </div>
  );
}
