// "Morning delivery" in the left ear of the masthead.
//  - Deliver daily at [hour]: registers a Windows Task Scheduler task that prints the
//    edition to screen before you open the app. Picking a time turns it on
//    (the job itself is the only place the time is stored).
//  - ...and on paper: after that morning run, send it to the printer too.
// Nothing to paste into a terminal.

import { useState } from "react";
import type { PrintStatus, ScheduleInfo } from "../types";
import { setPrintDaily, setSchedule } from "../api";

const HOURS = Array.from({ length: 24 }, (_, h) => h);
const label = (h: number) => `${h % 12 === 0 ? 12 : h % 12} ${h < 12 ? "AM" : "PM"}`;

interface Props {
  schedule: ScheduleInfo | null;
  onChange: (s: ScheduleInfo) => void;
  print: PrintStatus | null;
  onPrint: (p: PrintStatus) => void;
}

export default function Delivery({ schedule, onChange, print, onPrint }: Props) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!schedule || !schedule.supported) return null;

  const run = async (job: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await job();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const apply = (enabled: boolean, hour: number) => run(async () => onChange(await setSchedule(enabled, hour, 0)));
  const paper = (enabled: boolean) => run(async () => onPrint(await setPrintDaily(enabled)));

  const paperTip = print?.problem
    ? print.problem
    : `After the morning run, print the paper edition${print?.printer ? ` on ${print.printer}` : ""}. Black & white, two-sided, 8 pages at most, once a day.`;

  return (
    <div className="delivery">
      <div className="delivery-row" title="Prints a fresh edition in the background every day at this time, so it's ready when you open the app. If the PC is asleep or off, it runs when it's back.">
        <label className="toggle small">
          <input type="checkbox" checked={schedule.enabled} disabled={busy} onChange={(e) => void apply(e.target.checked, schedule.hour)} />
          <span>Deliver daily at</span>
        </label>
        <select value={schedule.hour} disabled={busy} onChange={(e) => void apply(true, Number(e.target.value))}>
          {HOURS.map((h) => (
            <option key={h} value={h}>
              {label(h)}
            </option>
          ))}
        </select>
      </div>
      {print && (
        <div className="delivery-row" title={paperTip}>
          <label className={`toggle small ${schedule.enabled ? "" : "dim"}`}>
            <input type="checkbox" checked={print.printDaily} disabled={busy || !schedule.enabled} onChange={(e) => void paper(e.target.checked)} />
            <span>and on paper{print.printDaily && print.problem ? " (!)" : ""}</span>
          </label>
        </div>
      )}
      {(error || (print?.printDaily && print.problem)) && <div className="delivery-error">{error ?? print?.problem}</div>}
    </div>
  );
}
