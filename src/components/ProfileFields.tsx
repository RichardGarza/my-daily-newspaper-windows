// Name, city, ball team: shared by the welcome page and Edit Interests.

import { MLB_TEAMS } from "../teams";

export interface ProfileDraft {
  ownerName: string;
  city: string;
  mlbTeamId: number;
}

interface Props {
  value: ProfileDraft;
  onChange: (next: ProfileDraft) => void;
  autoFocus?: boolean;
}

export default function ProfileFields({ value, onChange, autoFocus }: Props) {
  return (
    <div className="profile-fields">
      <label className="field grow">
        <span>Your first name</span>
        <input
          className="beat-name"
          value={value.ownerName}
          placeholder="It goes on the masthead"
          autoFocus={autoFocus}
          maxLength={40}
          onChange={(e) => onChange({ ...value, ownerName: e.target.value })}
        />
      </label>
      <label className="field grow">
        <span>City for the dateline (optional)</span>
        <input value={value.city} placeholder="e.g. Boise" maxLength={60} onChange={(e) => onChange({ ...value, city: e.target.value })} />
      </label>
      <label className="field">
        <span>Ball team for the score box</span>
        <select value={value.mlbTeamId} onChange={(e) => onChange({ ...value, mlbTeamId: Number(e.target.value) })}>
          <option value={0}>I’m lame and don’t like baseball</option>
          {MLB_TEAMS.map((t) => (
            <option key={t.id} value={t.id}>
              {t.name}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
