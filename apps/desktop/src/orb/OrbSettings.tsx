// Deck Settings for the edge orb: the off switch, reveal dwell and Focus level, the mixer rows (Game fixed first, up
// to three more: Kohbor, Music or any app with a sound session), plus a keyboard-reachable twin of every orb panel
// control (UI-006 rule): a level slider with mute per channel, the music app, Focus mode and Reset.
import { useCallback, useEffect, useState } from "react";
import { hostAvailable, mixerApi, orbApi, subscribeMixer } from "./orbApi";
import {
  appLabel, channelLabel, controllable, DWELL_CHOICES_MS, FOCUS_CHOICES, focusTargets, hideReasonText, MAX_SLOTS, moveSlot,
  orbCopy, parseOrbPrefs, reasonText, setSlot, slotChoices, slotKey, slotText, valueText,
  type ChannelId, type ChannelReading, type MixerReply, type MixerView, type OrbLocale, type OrbPrefsView, type SlotSpec,
} from "./orbModel";
import "./orbSettings.css";

const REFRESH_MS = 2000;

const settingsCopy = {
  th: {
    title: "วงกลมขอบจอ · มิกเซอร์เสียงในเกม",
    enable: "เปิดใช้วงกลมขอบจอซ้าย (ปรับเสียงเกม เสียงพากย์ เพลง และแอปอื่นระหว่างเล่น)",
    enableNote: "ค่าเริ่มต้นคือปิด ยังไม่ได้ทดสอบในแมตช์จริง ถ้าคลิกแล้ว Dota เสียโฟกัส วงกลมจะซ่อนเองจนจบแมตช์",
    dwell: "ชี้ค้างก่อนวงกลมออกมา", seconds: (ms: number) => `${(ms / 1000).toFixed(1)} วินาที`,
    focusLevel: "ระดับเสียงอื่นในโหมดโฟกัส",
    focusNote: "โหมดโฟกัสลดเพลงและแอปที่เพิ่มไว้ ไม่ลดเสียงเกมและเสียงพากย์ (เสียงเตือนสำคัญยังได้ยินเท่าเดิม)",
    hotkey: "ปุ่มลัดโหมดโฟกัส: ปิด (รุ่นนี้ยังตั้งปุ่มลัดไม่ได้)",
    status: "สถานะ", running: "ทำงาน", off: "ปิดอยู่",
    counts: (interactions: number, activations: number) => `ใช้งาน ${interactions} ครั้ง · เสียโฟกัส ${activations} ครั้ง`,
    startError: (code: string) => `เปิดวงกลมไม่สำเร็จ (${code})`,
    windowsOnly: "ใช้ได้เฉพาะ Windows",
    rows: "แถวเสียงในแผง (สูงสุด 4 แถว)",
    rowsNote: "แถวแรกคือเกม (ย้ายหรือลบไม่ได้) เลือกแอปจากแอปที่กำลังเล่นเสียงอยู่ เช่น Discord",
    row: (n: number) => `แถว ${n}`, gameRow: "เกม (Dota 2) · ตายตัว",
    up: (n: number) => `เลื่อนแถว ${n} ขึ้น`, down: (n: number) => `เลื่อนแถว ${n} ลง`,
    mixer: "ระดับเสียง (ใช้แทนแผงในเกมได้ด้วยคีย์บอร์ด)",
    muted: "ปิดเสียง", musicApp: "แอปเพลง", focusOn: "เปิดโหมดโฟกัส", focusOff: "ปิดโหมดโฟกัส",
    recovery: (count: number) => `รอคืนระดับเสียงเดิม ${count} รายการ`,
    unavailable: "อ่านมิกเซอร์ไม่ได้",
  },
  en: {
    title: "Edge orb · in-game sound mixer",
    enable: "Enable the left-edge orb (game, announcer, music and other app volume while playing)",
    enableNote: "Off by default. Not yet tested in a real match. If a click takes focus from Dota, the orb hides until the match ends",
    dwell: "Hover time before the orb comes out", seconds: (ms: number) => `${(ms / 1000).toFixed(1)} s`,
    focusLevel: "Other sound level in Focus mode",
    focusNote: "Focus mode lowers music and the apps you added; it never lowers the game or the announcer (critical alerts stay as loud)",
    hotkey: "Focus mode hotkey: off (cannot be assigned in this build)",
    status: "Status", running: "Running", off: "Off",
    counts: (interactions: number, activations: number) => `${interactions} interactions · ${activations} focus losses`,
    startError: (code: string) => `The orb could not start (${code})`,
    windowsOnly: "Windows only",
    rows: "Sound rows in the panel (at most 4)",
    rowsNote: "The first row is the game (fixed). Pick apps from the ones playing sound now, such as Discord",
    row: (n: number) => `Row ${n}`, gameRow: "Game (Dota 2) · fixed",
    up: (n: number) => `Move row ${n} up`, down: (n: number) => `Move row ${n} down`,
    mixer: "Levels (keyboard twin of the in-game panel)",
    muted: "Muted", musicApp: "Music app", focusOn: "Turn Focus mode on", focusOff: "Turn Focus mode off",
    recovery: (count: number) => `${count} saved level(s) waiting to be restored`,
    unavailable: "The mixer cannot be read",
  },
} as const;

function ChannelRow({ reading, locale, onLevel, onMute }: {
  reading: ChannelReading; locale: OrbLocale;
  onLevel: (id: ChannelId, percent: number) => void; onMute: (id: ChannelId, muted: boolean) => void;
}) {
  const enabled = controllable(reading);
  const [draft, setDraft] = useState<number | null>(null);
  const { name, sub } = channelLabel(reading, locale);
  const value = draft ?? reading.percent ?? reading.range?.[1] ?? 0;
  return (
    <div className="orb-settings-row">
      <label>
        <span>{name}{reading.kind === "music" && reading.exe ? ` · ${sub}` : ""}</span>
        <input type="range" min={0} max={100} step={5} value={value} disabled={!enabled} aria-valuetext={valueText(reading, locale)}
          onChange={(event) => setDraft(Number(event.currentTarget.value))}
          onPointerUp={() => { if (draft !== null) { onLevel(reading.id, draft); setDraft(null); } }}
          onKeyUp={() => { if (draft !== null) { onLevel(reading.id, draft); setDraft(null); } }} />
        <output>{draft !== null ? `${draft}%` : valueText(reading, locale)}</output>
      </label>
      <label className="orb-settings-mute">
        <input type="checkbox" checked={reading.muted === true} disabled={!enabled || (reading.kind === "kohbor" && reading.muted === null)}
          onChange={(event) => onMute(reading.id, event.currentTarget.checked)} />
        <span>{settingsCopy[locale].muted}</span>
      </label>
    </div>
  );
}

function SlotEditor({ slots, appChoices, locale, busy, onChange }: {
  slots: SlotSpec[]; appChoices: string[]; locale: OrbLocale; busy: boolean; onChange: (slots: SlotSpec[]) => void;
}) {
  const text = settingsCopy[locale];
  const rows = Array.from({ length: MAX_SLOTS }, (_, index) => index);
  return (
    <div className="orb-settings-slots">
      <h3>{text.rows}</h3>
      <p className="orb-settings-note">{text.rowsNote}</p>
      <ol>
        <li><span className="orb-settings-slot-n">1</span><span>{text.gameRow}</span></li>
        {rows.map((index) => {
          const current = slotKey(slots[index]);
          // An empty row can only be filled right after the last used row.
          const reachable = index <= slots.length;
          return (
            <li key={index}>
              <span className="orb-settings-slot-n">{index + 2}</span>
              <select aria-label={text.row(index + 2)} value={current} disabled={busy || !reachable}
                onChange={(event) => onChange(setSlot(slots, index, event.currentTarget.value))}>
                {slotChoices(slots, index, appChoices).map((key) => <option key={key} value={key}>{slotText(key, locale)}</option>)}
              </select>
              <button type="button" aria-label={text.up(index + 2)} disabled={busy || index === 0 || index >= slots.length}
                onClick={() => onChange(moveSlot(slots, index, -1))}>↑</button>
              <button type="button" aria-label={text.down(index + 2)} disabled={busy || index >= slots.length - 1}
                onClick={() => onChange(moveSlot(slots, index, 1))}>↓</button>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

export function OrbSettings({ locale }: { locale: OrbLocale }) {
  const text = settingsCopy[locale];
  const labels = orbCopy[locale];
  const [prefs, setPrefs] = useState<OrbPrefsView | null>(null);
  const [view, setView] = useState<MixerView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const available = hostAvailable();

  const loadPrefs = useCallback(() => orbApi.prefs().then((value) => setPrefs(parseOrbPrefs(value))).catch(() => setPrefs(null)), []);
  const apply = useCallback((reply: MixerReply) => { setView(reply.mixer); setError(reply.ok ? null : reply.code); }, []);
  const run = useCallback((call: Promise<MixerReply>) => {
    void call.then(apply).catch((reason: unknown) => setError(typeof reason === "string" ? reason : "MIXER_UNAVAILABLE"));
  }, [apply]);

  useEffect(() => {
    if (!available) return;
    void loadPrefs();
    let live = true;
    const read = () => mixerApi.get().then((reply) => { if (live) apply(reply); }).catch(() => { if (live) setView(null); });
    void read();
    const timer = window.setInterval(() => { void read(); void loadPrefs(); }, REFRESH_MS);
    let stop: (() => void) | undefined;
    void subscribeMixer(apply).then((unlisten) => { if (live) stop = unlisten; else unlisten(); });
    return () => { live = false; window.clearInterval(timer); stop?.(); };
  }, [available, apply, loadPrefs]);

  const patch = (change: Record<string, unknown>) => {
    setBusy(true);
    void orbApi.setPrefs(change).then((value) => { setPrefs(parseOrbPrefs(value)); setError(null); })
      .catch((reason: unknown) => setError(typeof reason === "string" ? reason : "ORB_PREFS_WRITE_FAILED"))
      .finally(() => { setBusy(false); void mixerApi.get().then(apply).catch(() => {}); });
  };

  if (!available) return null;
  const status = prefs?.status;
  const statusText = !prefs ? null : status?.start_error ? text.startError(status.start_error)
    : status?.windows_only ? text.windowsOnly
    : !prefs.enabled ? text.off
    : status?.running ? (hideReasonText(status.reason, locale) ?? text.running) : text.off;
  const music = view?.channels.find((reading) => reading.kind === "music") ?? null;
  const appSlots = new Set(view?.channels.filter((reading) => reading.kind === "app").map((reading) => reading.exe));
  const musicChoices = view?.app_choices.filter((exe) => !appSlots.has(exe)) ?? [];
  const singlePrior = view?.focus.channels.length === 1 ? view.focus.channels[0]?.prior_percent ?? null : null;
  return (
    <section className="card orb-settings" aria-labelledby="orb-settings-heading">
      <h2 id="orb-settings-heading">{text.title}</h2>
      <label className="orb-settings-toggle">
        <input type="checkbox" checked={prefs?.enabled === true} disabled={!prefs || busy} onChange={(event) => patch({ enabled: event.currentTarget.checked })} />
        <span>{text.enable}</span>
      </label>
      <p className="orb-settings-note">{text.enableNote}</p>
      <div className="orb-settings-grid">
        <label>
          <span>{text.dwell}</span>
          <select value={prefs?.dwell_ms ?? 1000} disabled={!prefs || busy} onChange={(event) => patch({ dwell_ms: Number(event.currentTarget.value) })}>
            {DWELL_CHOICES_MS.map((ms) => <option key={ms} value={ms}>{text.seconds(ms)}</option>)}
          </select>
        </label>
        <label>
          <span>{text.focusLevel}</span>
          <select value={prefs?.focus_percent ?? 20} disabled={!prefs || busy} onChange={(event) => patch({ focus_percent: Number(event.currentTarget.value) })}>
            {FOCUS_CHOICES.map((percent) => <option key={percent} value={percent}>{percent}%</option>)}
          </select>
        </label>
      </div>
      <p className="orb-settings-note">{text.focusNote}</p>
      <p className="orb-settings-note">{text.hotkey}</p>
      {statusText && (
        <p className="orb-settings-status" role="status">
          {text.status}: {statusText}{status && prefs?.enabled ? ` · ${text.counts(status.interactions, status.activations)}` : ""}
        </p>
      )}

      {prefs && (
        <SlotEditor slots={prefs.slots} appChoices={view?.app_choices ?? []} locale={locale} busy={busy}
          onChange={(slots) => patch({ slots })} />
      )}

      <h3>{text.mixer}</h3>
      {!view && <p className="orb-settings-note">{text.unavailable}</p>}
      {view?.channels.map((reading) => (
        <ChannelRow key={reading.id} reading={reading} locale={locale}
          onLevel={(id, percent) => run(mixerApi.level(id, percent))} onMute={(id, muted) => run(mixerApi.mute(id, muted))} />
      ))}
      <div className="orb-settings-actions">
        {music && (
          <label>
            <span>{text.musicApp}</span>
            <select value={music.exe ?? ""} disabled={musicChoices.length === 0}
              onChange={(event) => { if (event.currentTarget.value) run(mixerApi.app(event.currentTarget.value)); }}>
              {!music.exe && <option value="">—</option>}
              {music.exe && !musicChoices.includes(music.exe) && <option value={music.exe}>{appLabel(music.exe)}</option>}
              {musicChoices.map((exe) => <option key={exe} value={exe}>{appLabel(exe)}</option>)}
            </select>
          </label>
        )}
        <button type="button" disabled={!view || (focusTargets(view).length === 0 && !view.focus.active)} aria-pressed={view?.focus.active === true}
          onClick={() => run(mixerApi.focus())}>
          {view?.focus.active ? text.focusOff : text.focusOn} ({view?.focus.active ? labels.focusOn(singlePrior) : labels.focusOff(view?.focus.percent ?? 20)})
        </button>
        <button type="button" disabled={!view} onClick={() => run(mixerApi.reset())}>{labels.reset}</button>
      </div>
      <p className="orb-settings-note">{labels.resetNote}</p>
      {view && view.restore_pending > 0 && <p className="orb-settings-note">{text.recovery(view.restore_pending)}</p>}
      {error && <p className="orb-settings-error" role="alert">{reasonText(error, locale)}</p>}
    </section>
  );
}
