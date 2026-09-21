// First launch: who is this paper for? Then on to the interests page, which
// arrives pre-filled with a starter list to edit, keep, or skip for later.

import { useState } from "react";
import type { Profile } from "../types";
import { setProfile } from "../api";
import ProfileFields, { type ProfileDraft } from "./ProfileFields";

interface Props {
  profile: Profile;
  onDone: (p: Profile) => void;
}

export default function Welcome({ profile, onDone }: Props) {
  const [draft, setDraft] = useState<ProfileDraft>({
    ownerName: profile.ownerName,
    city: profile.city,
    mlbTeamId: profile.mlbTeamId,
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const name = draft.ownerName.trim();
  const go = async () => {
    setSaving(true);
    setError(null);
    try {
      // not "onboarded" yet: the interests page finishes the tour
      onDone(await setProfile({ ...draft, onboarded: false }));
    } catch (e) {
      setError(String(e));
      setSaving(false);
    }
  };

  return (
    <main className="page desk-page welcome">
      <div className="kicker">Welcome, new subscriber</div>
      <h1>{name ? `${name}’s Daily` : "Your own daily newspaper"}</h1>
      <p className="standfirst">
        Every morning Claude researches the things you care about, writes them up as short newspaper stories, and lays out a front page with
        exactly one subscriber: you. Three quick questions, then your beats.
      </p>
      {error && <div className="form-error">{error}</div>}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void go();
        }}
      >
        <ProfileFields value={draft} onChange={setDraft} autoFocus />
        <div className="desk-toolbar bottom">
          <span className="count">You can change all of this later under Edit Interests.</span>
          <button className="btn primary" type="submit" disabled={saving}>
            {saving ? "Saving…" : "Next: your interests"}
          </button>
        </div>
      </form>
    </main>
  );
}
