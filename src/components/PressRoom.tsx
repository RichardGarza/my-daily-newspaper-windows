// The right-hand "ear" of the masthead: a live readout of what the app is
// doing. Not a popup - it sits beside the nameplate like a weather box.
//
// While printing it shows: percent done, a bar, total time so far, and a rough
// time left. The percent is an estimate: each stage has a floor and a ceiling,
// and inside a stage it advances with the clock, scaled to how long the last
// edition took.

import { useEffect, useRef, useState } from "react";
import type { Edition, Status } from "../types";
import { clock, elapsed } from "../util";

interface Props {
  refreshing: boolean;
  log: Status[];
  startedAt: number | null;
  edition: Edition | null;
  error: string | null;
}

const DEFAULT_SECS = 150;

export function estimatePercent(log: Status[], startedAt: number, now: number, expectedSecs: number): number {
  const total = Math.max(45, expectedSecs);
  const secs = (now - startedAt) / 1000;
  const lookups = log.filter((s) => s.stage === "research" && /researching|reading/i.test(s.message)).length;
  const writingAt = log.find((s) => s.stage === "writing")?.at;
  const last = log[log.length - 1];

  if (last?.stage === "done") return 100;
  if (last?.stage === "content") return 96;
  if (last?.stage === "writing" && /copy desk/i.test(last.message)) return 90;
  if (writingAt) {
    // writing is the long quiet stretch: ease from 45 toward 88
    const t = (now - writingAt) / 1000;
    return 45 + 43 * (1 - Math.exp(-t / (total * 0.35)));
  }
  if (lookups > 0 || last?.stage === "research") {
    const bySteps = 12 + Math.min(lookups, 16) * 2;
    const byClock = (secs / total) * 100;
    return Math.min(45, Math.max(bySteps, byClock));
  }
  return Math.min(10, 2 + secs * 1.5);
}

export default function PressRoom({ refreshing, log, startedAt, edition, error }: Props) {
  const [, tick] = useState(0);
  const peak = useRef(0);
  useEffect(() => {
    if (!refreshing) {
      peak.current = 0;
      return;
    }
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [refreshing]);

  const current = log[log.length - 1];
  const earlier = log.slice(-2, -1);

  if (refreshing) {
    const now = Date.now();
    const expected = edition?.stats.durationSecs || DEFAULT_SECS;
    const raw = startedAt ? estimatePercent(log, startedAt, now, expected) : 0;
    peak.current = Math.max(peak.current, Math.min(99, raw)); // never runs backwards
    const pct = Math.round(peak.current);
    const spent = startedAt ? now - startedAt : 0;
    const leftMs = pct > 3 ? Math.max(0, (spent / pct) * (100 - pct)) : expected * 1000;
    const left = leftMs < 10_000 ? "almost done" : `~${elapsed(Math.round(leftMs / 5000) * 5000)} left`;

    return (
      <aside className="ear ear-right press live" aria-live="polite">
        <div className="ear-label">
          <span className="pulse" aria-hidden="true" />
          Press Room
          <span className="ear-clock">{pct}%</span>
        </div>
        <div className="progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
          <span style={{ width: `${pct}%` }} />
        </div>
        <div className="press-times">
          <span>{elapsed(spent)} so far</span>
          <span>{left}</span>
        </div>
        <div className="press-now">
          {current ? current.message : "Starting"}
          <span className="dots" aria-hidden="true" />
        </div>
        {current?.detail && <div className="press-detail">{current.detail}</div>}
        {earlier.length > 0 && (
          <ul className="press-log">
            {earlier.map((s) => (
              <li key={s.at + s.message}>{s.detail ? `${s.message}: ${s.detail}` : s.message}</li>
            ))}
          </ul>
        )}
      </aside>
    );
  }

  if (error) {
    return (
      <aside className="ear ear-right press failed" aria-live="polite">
        <div className="ear-label">Press Room</div>
        <div className="press-now">Stopped the presses</div>
        <div className="press-detail">{error}</div>
      </aside>
    );
  }

  return (
    <aside className="ear ear-right press">
      <div className="ear-label">Press Room</div>
      {edition ? (
        <>
          <div className="press-now">Edition ready</div>
          <div className="press-detail">
            Printed {clock(edition.generatedAt)} · took {elapsed(edition.stats.durationSecs * 1000)}
          </div>
          <div className="press-detail">
            {edition.cards.length} stories · {edition.stats.searches} searches · {edition.stats.xPostsFromGrok} X posts
          </div>
        </>
      ) : (
        <div className="press-detail">No edition yet.</div>
      )}
    </aside>
  );
}
