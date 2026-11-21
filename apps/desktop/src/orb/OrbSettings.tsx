// Deck Settings for the edge orb: the off switch, reveal dwell and Focus level, plus a keyboard-reachable twin of
// every orb panel control (UI-006 rule): three level sliders with mute, the music app, Focus mode and Reset.
import { useCallback, useEffect, useState } from "react";
import { hostAvailable, mixerApi, orbApi, subscribeMixer } from "./orbApi";
import {
  appLabel, controllable, DWELL_CHOICES_MS, FOCUS_CHOICES, hideReasonText, orbCopy, parseOrbPrefs, reasonText, valueText,
  type ChannelId, type MixerReply, type MixerView, type OrbLocale, type OrbPrefsView,
} from "./orbModel";
import "./orbSettings.css";

const REFRESH_MS = 2000;

const settingsCopy = {
  th: {
    title: "วงกลมขอบจอ · มิกเซอร์เสียงในเกม",
    enable: "เปิดใช้วงกลมขอบจอซ้าย (ปรับเสียงเกม เสียงพากย์ และเพลงระหว่างเล่น)",
    enableNote: "ค่าเริ่มต้นคือปิด ยังไม่ได้ทดสอบในแมตช์จริง ถ้าคลิกแล้ว Dota เสียโฟกัส วงกลมจะซ่อนเองจนจบแมตช์",
    dwell: "ชี้ค้างก่อนวงกลมออกมา", seconds: (ms: number) => `${(ms / 1000).toFixed(1)} วินาที`,
    focusLevel: "ระดับเพลงในโหมดโฟกัส",
    hotkey: "ปุ่มลัดโหมดโฟกัส: ปิด (รุ่นนี้ยังตั้งปุ่มลัดไม่ได้)",
    status: "สถานะ", running: "ทำงาน", off: "ปิดอยู่",
    counts: (interactions: number, activations: number) => `ใช้งาน ${interactions} ครั้ง · เสียโฟกัส ${activations} ครั้ง`,
    startError: (code: string) => `เปิดวงกลมไม่สำเร็จ (${code})`,
    windowsOnly: "ใช้ได้เฉพาะ Windows",
    mixer: "ระดับเสียง (ใช้แทนแผงในเกมได้ด้วยคีย์บอร์ด)",
    muted: "ปิดเสียง", musicApp: "แอปเพลง", focusOn: "เปิดโหมดโฟกัส", focusOff: "ปิดโหมดโฟกัส",
    recovery: (count: number) => `รอคืนระดับเสียงเดิม ${count} แอป`,
    unavailable: "อ่านมิกเซอร์ไม่ได้",
  },
  en: {
    title: "Edge orb · in-game sound mixer",
    enable: "Enable the left-edge orb (game, announcer and music volume while playing)",
    enableNote: "Off by default. Not yet tested in a real match. If a click takes focus from Dota, the orb hides until the match ends",
    dwell: "Hover time before the orb comes out", seconds: (ms: number) => `${(ms / 1000).toFixed(1)} s`,
    focusLevel: "Music level in Focus mode",
    hotkey: "Focus mode hotkey: off (cannot be assigned in this build)",
    status: "Status", running: "Running", off: "Off",
    counts: (interactions: number, activations: number) => `${interactions} interactions · ${activations} focus losses`,
    startError: (code: string) => `The orb could not start (${code})`,
    windowsOnly: "Windows only",
    mixer: "Levels (keyboard twin of the in-game panel)",
    muted: "Muted", musicApp: "Music app", focusOn: "Turn Focus mode on", focusOff: "Turn Focus mode off",
    recovery: (count: number) => `${count} app level(s) waiting to be restored`,
    unavailable: "The mixer cannot be read",
  },
} as const;

function ChannelRow({ id, name, reading, locale, onLevel, onMute }: {
  id: ChannelId; name: string; reading: MixerView["game"] | null; locale: OrbLocale;
  onLevel: (id: ChannelId, percent: number) => void; onMute: (id: ChannelId, muted: boolean) => void;
}) {
  const enabled = controllable(reading);
  const [draft, setDraft] = useState<number | null>(null);
  const value = draft ?? reading?.percent ?? 0;
  return (
    <div className="orb-settings-row">
      <label>
        <span>{name}{reading?.exe && id === "music" ? ` · ${appLabel(reading.exe)}` : ""}</span>
        <input type="range" min={0} max={100} step={5} value={value} disabled={!enabled} aria-valuetext={valueText(reading, locale)}
          onChange={(event) => setDraft(Number(event.currentTarget.value))}
          onPointerUp={() => { if (draft !== null) { onLevel(id, draft); setDraft(null); } }}
          onKeyUp={() => { if (draft !== null) { onLevel(id, draft); setDraft(null); } }} />
        <output>{draft !== null ? `${draft}%` : valueText(reading, locale)}</output>
      </label>
      <label className="orb-settings-mute">
        <input type="checkbox" checked={reading?.muted === true} disabled={!enabled || reading?.muted === null}
          onChange={(event) => onMute(id, event.currentTarget.checked)} />
        <span>{settingsCopy[locale].muted}</span>
      </label>
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
    void orbApi.setPrefs(change).then((value) => setPrefs(parseOrbPrefs(value)))
      .catch((reason: unknown) => setError(typeof reason === "string" ? reason : "ORB_PREFS_WRITE_FAILED"))
      .finally(() => setBusy(false));
  };

  if (!available) return null;
  const status = prefs?.status;
  const statusText = !prefs ? null : status?.start_error ? text.startError(status.start_error)
    : status?.windows_only ? text.windowsOnly
    : !prefs.enabled ? text.off
    : status?.running ? (hideReasonText(status.reason, locale) ?? text.running) : text.off;
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
      <p className="orb-settings-note">{text.hotkey}</p>
      {statusText && (
        <p className="orb-settings-status" role="status">
          {text.status}: {statusText}{status && prefs?.enabled ? ` · ${text.counts(status.interactions, status.activations)}` : ""}
        </p>
      )}

      <h3>{text.mixer}</h3>
      {!view && <p className="orb-settings-note">{text.unavailable}</p>}
      <ChannelRow id="game" name={labels.game} reading={view?.game ?? null} locale={locale}
        onLevel={(id, percent) => run(mixerApi.level(id, percent))} onMute={(id, muted) => run(mixerApi.mute(id, muted))} />
      <ChannelRow id="kohbor" name={labels.kohbor} reading={view?.kohbor ?? null} locale={locale}
        onLevel={(id, percent) => run(mixerApi.level(id, percent))} onMute={(id, muted) => run(mixerApi.mute(id, muted))} />
      <ChannelRow id="music" name={labels.music} reading={view?.music ?? null} locale={locale}
        onLevel={(id, percent) => run(mixerApi.level(id, percent))} onMute={(id, muted) => run(mixerApi.mute(id, muted))} />
      <div className="orb-settings-actions">
        <label>
          <span>{text.musicApp}</span>
          <select value={view?.music.exe ?? ""} disabled={!view || view.music_choices.length === 0}
            onChange={(event) => { if (event.currentTarget.value) run(mixerApi.app(event.currentTarget.value)); }}>
            {!view?.music.exe && <option value="">—</option>}
            {view?.music.exe && !view.music_choices.includes(view.music.exe) && <option value={view.music.exe}>{appLabel(view.music.exe)}</option>}
            {view?.music_choices.map((exe) => <option key={exe} value={exe}>{appLabel(exe)}</option>)}
          </select>
        </label>
        <button type="button" disabled={!view || (!controllable(view.music) && !view.focus.active)} aria-pressed={view?.focus.active === true}
          onClick={() => run(mixerApi.focus())}>
          {view?.focus.active ? text.focusOff : text.focusOn} ({view?.focus.active ? labels.focusOn(view.focus.prior_percent) : labels.focusOff(view?.focus.percent ?? 20)})
        </button>
        <button type="button" disabled={!view} onClick={() => run(mixerApi.reset())}>{labels.reset}</button>
      </div>
      <p className="orb-settings-note">{labels.resetNote}</p>
      {view && view.restore_pending > 0 && <p className="orb-settings-note">{text.recovery(view.restore_pending)}</p>}
      {error && <p className="orb-settings-error" role="alert">{reasonText(error, locale)}</p>}
    </section>
  );
}
