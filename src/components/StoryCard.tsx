import { useState } from "react";
import type { Card } from "../types";
import { ago } from "../util";
import { openReader } from "../api";

interface Props {
  card: Card;
  /** Force-hide the picture (tight text columns). */
  noImage?: boolean;
  /** Sits in the wide centre column: copy runs in two sub-columns. */
  wide?: boolean;
}

const MORE_LABEL: Record<Card["kind"], string> = {
  article: "Read more…",
  video: "See the video…",
  post: "See the post…",
};

function paragraphs(text?: string | null): string[] {
  return (text ?? "")
    .split(/\n\s*\n/)
    .map((p) => p.trim())
    .filter(Boolean);
}

export default function StoryCard({ card, noImage, wide }: Props) {
  const [imgOk, setImgOk] = useState(true);
  const openUrl = (url: string) => (e: React.MouseEvent) => {
    e.preventDefault();
    void openReader(url, card.headline);
  };
  const open = openUrl(card.url);

  const copy = paragraphs(card.story);
  const hasCopy = copy.length > 0;
  const showImage = !noImage && imgOk && !!card.image && (card.size !== "brief" || card.kind === "video");
  const when = ago(card.published);

  // An article about a video (or a video with a write-up) offers both doors.
  const second =
    card.kind === "article" && card.videoUrl
      ? { url: card.videoUrl, label: MORE_LABEL.video }
      : card.kind === "video" && card.articleUrl
        ? { url: card.articleUrl, label: MORE_LABEL.article }
        : null;
  const more = (
    <div className="more-row">
      <a href={card.url} onClick={open} className="more">
        {MORE_LABEL[card.kind]}
      </a>
      {second && (
        <a href={second.url} onClick={openUrl(second.url)} className="more">
          {second.label}
        </a>
      )}
    </div>
  );

  if (card.kind === "post") {
    return (
      <article className={`story post ${wide ? "wide" : ""}`}>
        <a href={card.url} onClick={open} className="post-text">
          {card.dek || card.headline}
        </a>
        <div className="byline">
          <span className="src">{card.author || "X"}</span>
          {when && <span className="when">{when}</span>}
        </div>
        {hasCopy && (
          <div className="copy">
            {copy.map((p, i) => (
              <p key={i}>{p}</p>
            ))}
          </div>
        )}
        {more}
      </article>
    );
  }

  // With a written story, a brief's one-line dek is redundant; bigger pieces
  // keep it as the standfirst.
  const showDek = !!card.dek && (!hasCopy || card.size !== "brief");

  return (
    <article className={`story ${card.size} ${card.kind} ${hasCopy ? "has-copy" : ""} ${wide ? "wide" : ""}`}>
      {showImage && (
        <a href={card.url} onClick={open} className="figure" tabIndex={-1} aria-hidden="true">
          <img src={card.image!} alt="" loading="lazy" referrerPolicy="no-referrer" onError={() => setImgOk(false)} />
          {card.kind === "video" && <span className="play" />}
        </a>
      )}
      <div className="story-body">
        {card.kind === "video" && <div className="kicker">Video</div>}
        <h3 className="headline">
          <a href={card.url} onClick={open}>
            {card.headline}
          </a>
        </h3>
        {showDek && <p className="dek">{card.dek}</p>}
        <div className="byline">
          <span className="src">{card.source}</span>
          {when && <span className="when">{when}</span>}
        </div>
        {hasCopy && (
          <div className="copy">
            {copy.map((p, i) => (
              <p key={i}>{p}</p>
            ))}
          </div>
        )}
        {more}
        {card.why && !hasCopy && <div className="why">{card.why}</div>}
      </div>
    </article>
  );
}
