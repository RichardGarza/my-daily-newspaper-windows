// The score bug in the top bar, for whichever ball team the owner picked
// (none = no bug). During a game: score, inning, count, outs, who's on base,
// and ABS challenges left when the league reports them. Otherwise: the last
// final, and the next game with its time and where to watch it.
// Polls every 30 s during a game, every 5 min otherwise.

import { useEffect, useRef, useState } from "react";
import type { GameLine, ScoreBugData, Situation } from "../types";
import { openReader, scoreBug } from "../api";

function dayLabel(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  const today = new Date();
  const diff = Math.round((new Date(d.toDateString()).getTime() - new Date(today.toDateString()).getTime()) / 86400000);
  if (diff === 0) return "Today";
  if (diff === -1) return "Yesterday";
  if (diff === 1) return "Tomorrow";
  if (diff > 1 && diff < 7) return d.toLocaleDateString("en-US", { weekday: "long" });
  return d.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric" });
}

/** "Dodgers vs. Padres · Today 5:10 PM PDT · SportsNet LA" */
export function nextGameLine(g: GameLine): string {
  const d = new Date(g.start);
  const time = d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZoneName: "short" });
  const matchup = g.weAreHome ? `${g.home.name} vs. ${g.away.name}` : `${g.away.name} at ${g.home.name}`;
  const tv = g.tv && g.tv.length > 0 ? ` · ${g.tv.join(", ")}` : "";
  return `${matchup} · ${dayLabel(g.start)} ${time}${tv}`;
}

function Diamond({ bases }: { bases: Situation["bases"] }) {
  // second on top, third left, first right
  const spot = (on: boolean, x: number, y: number) => (
    <rect x={x} y={y} width="7" height="7" transform={`rotate(45 ${x + 3.5} ${y + 3.5})`} className={on ? "on" : ""} />
  );
  const label = ["first", "second", "third"].filter((_, i) => bases[i]).join(", ");
  return (
    <svg className="diamond" viewBox="0 0 30 22" width="30" height="22" role="img" aria-label={label ? `Runners on ${label}` : "Bases empty"}>
      {spot(bases[1], 11.5, 1.5)}
      {spot(bases[2], 2.5, 10.5)}
      {spot(bases[0], 20.5, 10.5)}
    </svg>
  );
}

function Outs({ n }: { n: number }) {
  return (
    <span className="outs" aria-label={`${n} out`}>
      {[0, 1, 2].map((i) => (
        <i key={i} className={i < n ? "on" : ""} />
      ))}
    </span>
  );
}

export default function ScoreBug({ teamId }: { teamId: number }) {
  const [data, setData] = useState<ScoreBugData | null>(null);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!teamId) {
      setData(null);
      return;
    }
    let alive = true;
    const load = async () => {
      let live = false;
      try {
        const d = await scoreBug();
        if (!alive) return;
        setData(d);
        live = d.game?.state === "live";
      } catch {
        /* no network, no bug - try again later */
      }
      if (alive) timer.current = window.setTimeout(load, live ? 30_000 : 300_000);
    };
    void load();
    return () => {
      alive = false;
      window.clearTimeout(timer.current);
    };
  }, [teamId]);

  if (!teamId) return null;
  const g = data?.game;
  const next = data?.next;
  if (!g && !next) return null;

  const live = g?.state === "live";
  const sit = live ? g?.situation : null;
  const awayWon = g?.state === "final" && (g.away.score ?? 0) > (g.home.score ?? 0);
  const homeWon = g?.state === "final" && (g.home.score ?? 0) > (g.away.score ?? 0);
  const target = g ?? next!;
  const title = sit
    ? [sit.batter && `At bat: ${sit.batter}`, sit.pitcher && `Pitching: ${sit.pitcher}`].filter(Boolean).join(" · ") || "Open the game"
    : "Open the box score";

  return (
    <button className={`scorebug ${live ? "live" : ""}`} onClick={() => void openReader(target.url, `${target.away.name} at ${target.home.name}`)} title={title}>
      {g && (
        <>
          <span className="sb-status">
            {live && <span className="pulse" aria-hidden="true" />}
            {live ? g.detail : `${g.detail} · ${dayLabel(g.start)}`}
          </span>
          <span className={`sb-team ${awayWon ? "won" : ""}`}>
            {g.away.abbr} <b>{g.away.score ?? "-"}</b>
          </span>
          <span className="sb-at">@</span>
          <span className={`sb-team ${homeWon ? "won" : ""}`}>
            {g.home.abbr} <b>{g.home.score ?? "-"}</b>
          </span>
        </>
      )}
      {sit && (
        <span className="sb-situation">
          <Diamond bases={sit.bases} />
          <span className="sb-count" aria-label={`${sit.balls} balls, ${sit.strikes} strikes`}>
            {sit.balls}-{sit.strikes}
          </span>
          <Outs n={sit.outs} />
          {sit.challenges && (
            <span className="sb-abs" title="ABS challenges left (away / home)">
              ABS {sit.challenges[0]}/{sit.challenges[1]}
            </span>
          )}
        </span>
      )}
      {!live && next && <span className="sb-next">Next: {nextGameLine(next)}</span>}
    </button>
  );
}
