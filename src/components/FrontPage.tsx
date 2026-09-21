// The front page and the desks under it.
//
// No holes in the columns:
//  - Front page: three columns (side, wide centre with the lead, side). Every
//    other front-page story is dropped into whichever column is currently the
//    shortest, using a height estimate from its character counts, so the three
//    columns finish close to level.
//  - Desks: real newspaper flow. Stories run down balanced CSS columns and may
//    continue at the top of the next one, so every column ends at the same line.

import type { Card, Edition } from "../types";
import { slug } from "../util";
import StoryCard from "./StoryCard";

const sizeRank: Record<string, number> = { lead: 0, feature: 1, brief: 2 };

const chars = (s?: string | null) => (s ? s.length : 0);

/** Rough rendered height in px. `wide` = the centre column, where copy runs in two sub-columns. */
export function estimateHeight(c: Card, wide: boolean, withImage: boolean): number {
  const CPL = 42; // characters per line in one text column
  const LINE = 21.75;
  const copyLines = Math.ceil(chars(c.story) / CPL) + (c.story ? c.story.split(/\n\s*\n/).length : 0) * 0.3;
  const copy = (wide ? copyLines / 2 : copyLines) * LINE;
  if (c.kind === "post") return Math.ceil(chars(c.dek) / CPL) * 22 + copy + 95;
  const lead = c.size === "lead";
  const headCpl = lead ? 30 : wide ? 52 : c.size === "feature" ? 24 : 30;
  const headLine = lead ? 43 : c.size === "feature" ? 25 : 20;
  const headline = Math.ceil(chars(c.headline) / headCpl) * headLine;
  const showDek = !!c.dek && (!c.story || c.size !== "brief");
  const dek = showDek ? Math.ceil(chars(c.dek) / (wide ? 90 : CPL)) * (lead ? 24 : 21) + 8 : 0;
  const image = withImage && c.image && (c.size !== "brief" || c.kind === "video") ? (wide ? 350 : 172) : 0;
  return image + headline + dek + copy + 30 /* byline */ + 42 /* button */ + 30 /* rule + padding */;
}

interface Placed {
  card: Card;
  noImage: boolean;
}

/** Lead in the centre; everything else to the shortest column. */
export function layoutFront(lead: Card, rest: Card[]): { left: Placed[]; center: Placed[]; right: Placed[] } {
  const cols: Array<{ items: Placed[]; h: number; wide: boolean }> = [
    { items: [], h: 0, wide: false },
    { items: [{ card: lead, noImage: false }], h: estimateHeight(lead, true, true), wide: true },
    { items: [], h: 0, wide: false },
  ];
  // Too few stories to fill three columns: let the lead have the width.
  if (rest.length === 0) return { left: [], center: cols[1].items, right: [] };

  const ordered = [...rest].sort((a, b) => sizeRank[a.size] - sizeRank[b.size]);
  for (const card of ordered) {
    // side columns carry one picture each at most
    const target = cols.reduce((best, c) => (c.h < best.h ? c : best), cols[0]);
    const hasPicture = target.items.some((p) => !p.noImage && !!p.card.image && (p.card.size !== "brief" || p.card.kind === "video"));
    const noImage = target.wide ? true : hasPicture;
    target.items.push({ card, noImage });
    target.h += estimateHeight(card, target.wide, !noImage);
  }
  return { left: cols[0].items, center: cols[1].items, right: cols[2].items };
}

export default function FrontPage({ edition }: { edition: Edition }) {
  const bySection = new Map<string, Card[]>();
  for (const name of edition.sections) bySection.set(name, []);
  for (const c of edition.cards) {
    if (!bySection.has(c.section)) bySection.set(c.section, []);
    bySection.get(c.section)!.push(c);
  }

  const lead = edition.cards.find((c) => c.size === "lead") ?? edition.cards[0];
  const frontName = lead?.section ?? "Front Page";
  const front = (bySection.get(frontName) ?? []).filter((c) => c.id !== lead?.id);
  const placed = lead ? layoutFront(lead, front) : null;
  const shape = !placed ? "" : placed.left.length === 0 && placed.right.length === 0 ? "solo" : placed.right.length === 0 || placed.left.length === 0 ? "duo" : "";

  const rest = [...bySection.entries()].filter(([name, cards]) => name !== frontName && cards.length > 0);

  return (
    <main className="page">
      {placed && (
        <section className={`front ${shape}`} id={slug(frontName)}>
          {placed.left.length > 0 && (
            <div className="front-col side">
              {placed.left.map((p) => (
                <StoryCard key={p.card.id} card={p.card} noImage={p.noImage} />
              ))}
            </div>
          )}
          <div className="front-col center">
            {placed.center.map((p) => (
              <StoryCard key={p.card.id} card={p.card} noImage={p.noImage} wide={p.card.size !== "lead"} />
            ))}
          </div>
          {placed.right.length > 0 && (
            <div className="front-col side">
              {placed.right.map((p) => (
                <StoryCard key={p.card.id} card={p.card} noImage={p.noImage} />
              ))}
            </div>
          )}
        </section>
      )}

      {rest.map(([name, cards]) => {
        const sorted = [...cards].sort((a, b) => sizeRank[a.size] - sizeRank[b.size]);
        const isPosts = sorted.every((c) => c.kind === "post");
        const cols = Math.min(4, Math.max(2, sorted.length + (sorted.length < 3 ? 1 : 0)));
        return (
          <section className="desk" key={name} id={slug(name)}>
            <header className="desk-head">
              <h2>{name}</h2>
            </header>
            <div className={isPosts ? "desk-posts" : "desk-flow"} style={isPosts ? undefined : ({ "--cols": cols } as React.CSSProperties)}>
              {sorted.map((c) => (
                <StoryCard key={c.id} card={c} />
              ))}
            </div>
          </section>
        );
      })}

      {edition.notes.length > 0 && (
        <section className="corrections">
          <h2>From the Press Room</h2>
          <ul>
            {edition.notes.map((n, i) => (
              <li key={i}>{n}</li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}
