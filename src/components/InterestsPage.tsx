// "The Assignment Desk": add, remove, edit, switch off and re-order beats.
// Order matters - it is the priority order the editors see.

import { useEffect, useMemo, useState } from "react";
import type { Interest, InterestKind, InterestsFile, Profile } from "../types";
import { getInterests, resetInterests, saveInterests, setProfile } from "../api";
import ProfileFields, { type ProfileDraft } from "./ProfileFields";

interface Props {
  onClose: (saved: boolean) => void;
  profile: Profile;
  onProfile: (p: Profile) => void;
  /** First launch: the list is a starter to edit, keep, or skip for later. */
  firstRun?: boolean;
}

const KINDS: Array<{ value: InterestKind; label: string }> = [
  { value: "topic", label: "Topic" },
  { value: "person", label: "Person" },
  { value: "youtube_channel", label: "YouTube channel" },
  { value: "x_account", label: "X account" },
];

const VALUE_HINT: Record<InterestKind, { label: string; placeholder: string }> = {
  topic: { label: "News search terms (optional)", placeholder: "Defaults to the name. OR works: Glock OR red dot" },
  person: { label: "News search terms (optional)", placeholder: "Defaults to the name" },
  youtube_channel: { label: "Channel handle or URL", placeholder: "@joerogan or https://www.youtube.com/@..." },
  x_account: { label: "X handle", placeholder: "@handle" },
};

/** Two-click button: native confirm() dialogs are unreliable inside webviews. */
function ConfirmButton({
  label,
  confirmLabel,
  onConfirm,
  className,
  disabled,
  armed: needsConfirm = true,
}: {
  label: string;
  confirmLabel: string;
  onConfirm: () => void;
  className?: string;
  disabled?: boolean;
  /** When false, a single click is enough. */
  armed?: boolean;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 3500);
    return () => clearTimeout(t);
  }, [armed]);
  return (
    <button
      className={`${className ?? "btn"} ${armed ? "armed" : ""}`}
      disabled={disabled}
      onClick={() => {
        if (!needsConfirm || armed) {
          setArmed(false);
          onConfirm();
        } else {
          setArmed(true);
        }
      }}
    >
      {armed ? confirmLabel : label}
    </button>
  );
}

let seq = 0;
const newInterest = (): Interest => ({
  id: `new-${Date.now()}-${seq++}`,
  name: "",
  kind: "topic",
  value: "",
  notes: "",
  enabled: true,
});

export default function InterestsPage({ onClose, profile, onProfile, firstRun }: Props) {
  const [file, setFile] = useState<InterestsFile | null>(null);
  const [original, setOriginal] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [justAdded, setJustAdded] = useState<string | null>(null);
  const [who, setWho] = useState<ProfileDraft>({ ownerName: profile.ownerName, city: profile.city, mlbTeamId: profile.mlbTeamId });
  const whoDirty = who.ownerName !== profile.ownerName || who.city !== profile.city || who.mlbTeamId !== profile.mlbTeamId;

  useEffect(() => {
    getInterests()
      .then((f) => {
        setFile(f);
        setOriginal(JSON.stringify(f.interests));
      })
      .catch((e) => setError(String(e)));
  }, []);

  const listDirty = useMemo(() => !!file && JSON.stringify(file.interests) !== original, [file, original]);
  const dirty = listDirty || whoDirty;

  useEffect(() => {
    if (!justAdded) return;
    const el = document.querySelector<HTMLInputElement>(`[data-row="${justAdded}"] input.beat-name`);
    el?.focus();
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
    setJustAdded(null);
  }, [justAdded, file]);

  if (!file) {
    return (
      <main className="page desk-page">
        <p className="empty-note">{error ?? "Opening the assignment desk…"}</p>
      </main>
    );
  }

  const list = file.interests;
  const update = (id: string, patch: Partial<Interest>) =>
    setFile({ ...file, interests: list.map((i) => (i.id === id ? { ...i, ...patch } : i)) });
  const remove = (id: string) => setFile({ ...file, interests: list.filter((i) => i.id !== id) });
  const move = (idx: number, dir: -1 | 1) => {
    const j = idx + dir;
    if (j < 0 || j >= list.length) return;
    const next = [...list];
    [next[idx], next[j]] = [next[j], next[idx]];
    setFile({ ...file, interests: next });
  };
  const add = (where: "top" | "bottom") => {
    const item = newInterest();
    setFile({ ...file, interests: where === "top" ? [item, ...list] : [...list, item] });
    setJustAdded(item.id);
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      if (listDirty) await saveInterests(file);
      if (whoDirty || firstRun) onProfile(await setProfile({ ...who, onboarded: true }));
      onClose(true);
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  const reset = async () => {
    try {
      const f = await resetInterests();
      setFile(f);
      setOriginal(JSON.stringify(f.interests));
    } catch (e) {
      setError(String(e));
    }
  };

  const cancel = () => onClose(false);
  const skip = async () => {
    try {
      onProfile(await setProfile({ ownerName: profile.ownerName, city: profile.city, mlbTeamId: profile.mlbTeamId, onboarded: true }));
    } catch {
      /* the tour can be skipped even if saving that fact fails */
    }
    onClose(true);
  };

  const active = list.filter((i) => i.enabled && i.name.trim()).length;

  return (
    <main className="page desk-page">
      <header className="desk-page-head">
        <div>
          <div className="kicker">{firstRun ? "Step 2 of 2 \u00b7 The Assignment Desk" : "The Assignment Desk"}</div>
          <h1>{firstRun ? "Your beats" : "Edit Interests"}</h1>
          <p className="standfirst">
            {firstRun
              ? "This is a starter list, the beats this paper was first built around. Rewrite it, switch things off, add your own, or skip it for now and come back under Edit Interests. Top of the list gets the most ink."
              : "These are the beats Claude and Grok cover for you. Top of the list gets the most ink. Changes take effect on the next Refresh."}
          </p>
        </div>
        <div className="desk-actions">
          {firstRun ? (
            <button className="btn" onClick={() => void skip()} disabled={saving}>
              Skip for now
            </button>
          ) : (
            <ConfirmButton
              label={dirty ? "Cancel" : "Back to front page"}
              confirmLabel="Discard changes?"
              onConfirm={cancel}
              disabled={saving}
              armed={dirty}
            />
          )}
          <button className="btn primary" onClick={save} disabled={saving || (!dirty && !firstRun)}>
            {saving ? "Saving\u2026" : firstRun ? "Save & print my first edition" : "Save"}
          </button>
        </div>
      </header>

      {!firstRun && (
        <section className="your-paper">
          <h2>Your paper</h2>
          <ProfileFields value={who} onChange={setWho} />
        </section>
      )}

      {error && <div className="form-error">{error}</div>}

      <div className="desk-toolbar">
        <button className="btn" onClick={() => add("top")}>
          + Add interest
        </button>
        <span className="count">
          {active} active · {list.length} total
        </span>
        <ConfirmButton className="btn quiet" label="Reset to starter list" confirmLabel="Replace your whole list?" onConfirm={reset} />
      </div>

      <ol className="beats">
        {list.map((it, idx) => {
          const hint = VALUE_HINT[it.kind] ?? VALUE_HINT.topic;
          return (
            <li key={it.id} data-row={it.id} className={`beat ${it.enabled ? "" : "off"}`}>
              <div className="beat-rank">
                <button className="arrow" onClick={() => move(idx, -1)} disabled={idx === 0} title="Move up" aria-label="Move up">
                  ▲
                </button>
                <span className="rank-no">{idx + 1}</span>
                <button
                  className="arrow"
                  onClick={() => move(idx, 1)}
                  disabled={idx === list.length - 1}
                  title="Move down"
                  aria-label="Move down"
                >
                  ▼
                </button>
              </div>

              <div className="beat-fields">
                <div className="row">
                  <label className="field grow">
                    <span>Interest</span>
                    <input
                      className="beat-name"
                      value={it.name}
                      placeholder="e.g. Starship launches"
                      onChange={(e) => update(it.id, { name: e.target.value })}
                    />
                  </label>
                  <label className="field">
                    <span>Type</span>
                    <select value={it.kind} onChange={(e) => update(it.id, { kind: e.target.value as InterestKind })}>
                      {KINDS.map((k) => (
                        <option key={k.value} value={k.value}>
                          {k.label}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
                <label className="field">
                  <span>{hint.label}</span>
                  <input value={it.value} placeholder={hint.placeholder} onChange={(e) => update(it.id, { value: e.target.value })} />
                </label>
                <label className="field">
                  <span>Notes to the editor (optional)</span>
                  <textarea
                    rows={2}
                    value={it.notes}
                    placeholder="What you want and what to skip. e.g. Long-form interviews only, no gossip."
                    onChange={(e) => update(it.id, { notes: e.target.value })}
                  />
                </label>
              </div>

              <div className="beat-side">
                <label className="toggle">
                  <input type="checkbox" checked={it.enabled} onChange={(e) => update(it.id, { enabled: e.target.checked })} />
                  <span>{it.enabled ? "On" : "Off"}</span>
                </label>
                <ConfirmButton
                  className="btn quiet danger"
                  label="Remove"
                  confirmLabel="Remove?"
                  onConfirm={() => remove(it.id)}
                  armed={it.name.trim() !== ""}
                />
              </div>
            </li>
          );
        })}
      </ol>

      {list.length === 0 && <p className="empty-note">No interests yet. Add one and the presses have something to print.</p>}

      {list.length > 3 && (
        <div className="desk-toolbar bottom">
          <button className="btn" onClick={() => add("bottom")}>
            + Add interest
          </button>
          <button className="btn primary" onClick={save} disabled={saving || (!dirty && !firstRun)}>
            {saving ? "Saving\u2026" : firstRun ? "Save & print my first edition" : "Save"}
          </button>
        </div>
      )}
    </main>
  );
}
