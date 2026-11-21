// Edge orb (UI-009) and orb sound mixer (UI-011): pure view logic shared by the orb window and its deck twins.
// The host decides reveal, hit regions and every audio value; this module only parses and formats what it sends.

export type OrbPhase = "hidden" | "tucked" | "charging" | "sliding" | "out" | "open";

export interface OrbRect { x: number; y: number; w: number; h: number }

export interface OrbLayout {
  disc: number;
  dead: number;
  cy: number;
  out_cx: number;
  hot: OrbRect;
  panel_x: number;
  panel_w: number;
  sp: number;
  monitor_h: number;
}

export interface OrbState {
  visible: boolean;
  phase: OrbPhase;
  reason: string | null;
  dwell_ms: number;
  charge_ms: number | null;
  locked: boolean;
  panel: OrbRect | null;
  layout: OrbLayout | null;
}

export type ChannelId = "game" | "kohbor" | "music";
export type SessionState = "present" | "missing" | "unreadable";

export interface ChannelReading {
  channel: ChannelId;
  state: SessionState;
  exe: string | null;
  percent: number | null;
  muted: boolean | null;
}

export interface MixerView {
  available: boolean;
  reason: string | null;
  game: ChannelReading;
  kohbor: ChannelReading;
  music: ChannelReading;
  music_choices: string[];
  music_selected: string | null;
  focus: { active: boolean; percent: number; prior_percent: number | null };
  restore_pending: number;
}

export interface MixerReply { ok: boolean; code: string | null; mixer: MixerView }

const PHASES: readonly OrbPhase[] = ["hidden", "tucked", "charging", "sliding", "out", "open"];
const CHANNELS: readonly ChannelId[] = ["game", "kohbor", "music"];
export const WHEEL_STEP = 5;
export const FOCUS_CHOICES = [10, 15, 20, 25, 30, 40, 50] as const;
export const DWELL_CHOICES_MS = [300, 600, 1000] as const;

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function rect(value: unknown): OrbRect | null {
  const r = record(value);
  if (!r || !finite(r.x) || !finite(r.y) || !finite(r.w) || !finite(r.h)) return null;
  return { x: r.x, y: r.y, w: r.w, h: r.h };
}

function layout(value: unknown): OrbLayout | null {
  const l = record(value);
  if (!l) return null;
  const hot = rect(l.hot);
  const keys = ["disc", "dead", "cy", "out_cx", "panel_x", "panel_w", "sp", "monitor_h"] as const;
  if (!hot || keys.some((key) => !finite(l[key]))) return null;
  return {
    disc: l.disc as number, dead: l.dead as number, cy: l.cy as number, out_cx: l.out_cx as number, hot,
    panel_x: l.panel_x as number, panel_w: l.panel_w as number, sp: l.sp as number, monitor_h: l.monitor_h as number,
  };
}

export const HIDDEN_ORB: OrbState = {
  visible: false, phase: "hidden", reason: null, dwell_ms: 1000, charge_ms: null, locked: false, panel: null, layout: null,
};

/** Parse `orb://state`. Anything malformed hides the orb (fail closed). */
export function parseOrbState(payload: unknown): OrbState {
  const p = record(payload);
  if (!p || typeof p.visible !== "boolean" || !PHASES.includes(p.phase as OrbPhase) || !finite(p.dwell_ms)) return HIDDEN_ORB;
  const phase = p.phase as OrbPhase;
  const parsedLayout = p.layout === null ? null : layout(p.layout);
  if (p.visible && !parsedLayout) return HIDDEN_ORB;
  return {
    visible: p.visible && phase !== "hidden",
    phase,
    reason: typeof p.reason === "string" ? p.reason : null,
    dwell_ms: p.dwell_ms,
    charge_ms: finite(p.charge_ms) ? p.charge_ms : null,
    locked: p.locked === true,
    panel: p.panel === null || p.panel === undefined ? null : rect(p.panel),
    layout: parsedLayout,
  };
}

/** The tucked disc grows linearly 80 -> 100 % (opacity 60 -> 100 %) over the dwell. Reduced motion: no scale. */
export function chargeCue(state: OrbState, elapsedSinceStateMs: number, reducedMotion: boolean): { scale: number; opacity: number; progress: number } {
  const out = state.phase === "sliding" || state.phase === "out" || state.phase === "open";
  if (out) return { scale: 1, opacity: 1, progress: 1 };
  if (state.phase !== "charging" || state.charge_ms === null) return { scale: 0.8, opacity: 0.6, progress: 0 };
  const dwell = Math.max(1, state.dwell_ms);
  const progress = Math.min(1, Math.max(0, (state.charge_ms + Math.max(0, elapsedSinceStateMs)) / dwell));
  if (reducedMotion) return { scale: 0.8, opacity: 0.6, progress };
  return { scale: 0.8 + 0.2 * progress, opacity: 0.6 + 0.4 * progress, progress };
}

/** Wheel over a fader: one notch is 5 %, snapped to the 5 % grid; wheel up is louder. */
export function wheelStep(current: number, deltaY: number): number {
  const value = Math.round(Math.min(100, Math.max(0, current)));
  if (deltaY === 0 || !Number.isFinite(deltaY)) return value;
  const next = deltaY < 0
    ? Math.floor(value / WHEEL_STEP) * WHEEL_STEP + WHEEL_STEP
    : Math.ceil(value / WHEEL_STEP) * WHEEL_STEP - WHEEL_STEP;
  return Math.min(100, Math.max(0, next));
}

/** Pointer height on a vertical fader track (bottom = 0 %). */
export function faderPercent(clientY: number, trackTop: number, trackHeight: number): number {
  if (!Number.isFinite(clientY) || !(trackHeight > 0)) return 0;
  return Math.round(Math.min(100, Math.max(0, ((trackTop + trackHeight - clientY) / trackHeight) * 100)));
}

function channel(value: unknown, id: ChannelId): ChannelReading | null {
  const c = record(value);
  if (!c || c.channel !== id) return null;
  const state = c.state;
  if (state !== "present" && state !== "missing" && state !== "unreadable") return null;
  const percent = finite(c.percent) && c.percent >= 0 && c.percent <= 100 ? Math.round(c.percent) : null;
  // A present channel must carry a real level; otherwise it is not controllable.
  if (state === "present" && percent === null) return null;
  return {
    channel: id,
    state,
    exe: typeof c.exe === "string" ? c.exe : null,
    percent: state === "present" ? percent : null,
    muted: state === "present" && typeof c.muted === "boolean" ? c.muted : null,
  };
}

export function parseMixer(value: unknown): MixerView | null {
  const m = record(value);
  if (!m || typeof m.available !== "boolean") return null;
  const parsed = CHANNELS.map((id) => channel(m[id], id));
  const [game, kohbor, music] = parsed;
  if (!game || !kohbor || !music) return null;
  const focus = record(m.focus);
  const choices = Array.isArray(m.music_choices) ? m.music_choices.filter((exe): exe is string => typeof exe === "string") : [];
  return {
    available: m.available,
    reason: typeof m.reason === "string" ? m.reason : null,
    game, kohbor, music,
    music_choices: choices,
    music_selected: typeof m.music_selected === "string" ? m.music_selected : null,
    focus: {
      active: focus?.active === true,
      percent: finite(focus?.percent) ? focus.percent as number : 20,
      prior_percent: finite(focus?.prior_percent) ? focus.prior_percent as number : null,
    },
    restore_pending: finite(m.restore_pending) ? m.restore_pending : 0,
  };
}

export function parseMixerReply(value: unknown): MixerReply | null {
  const r = record(value);
  if (!r || typeof r.ok !== "boolean") return null;
  const mixer = parseMixer(r.mixer);
  if (!mixer) return null;
  return { ok: r.ok, code: typeof r.code === "string" ? r.code : null, mixer };
}

export function controllable(reading: ChannelReading | null | undefined): boolean {
  return reading?.state === "present" && reading.percent !== null;
}

const APP_NAMES: Record<string, string> = {
  "chrome.exe": "Chrome", "msedge.exe": "Edge", "firefox.exe": "Firefox", "spotify.exe": "Spotify",
  "dota2.exe": "Dota 2", "discord.exe": "Discord", "brave.exe": "Brave", "opera.exe": "Opera",
};

export function appLabel(exe: string | null): string {
  if (!exe) return "—";
  const known = APP_NAMES[exe.toLowerCase()];
  if (known) return known;
  const base = exe.replace(/\.exe$/i, "");
  return base.charAt(0).toUpperCase() + base.slice(1);
}

export type OrbLocale = "th" | "en";

export const orbCopy = {
  th: {
    game: "เกม", gameSub: "Dota 2", kohbor: "เกาะเบาะ", kohborSub: "เสียงพากย์", music: "เพลง",
    change: "เปลี่ยน", focus: "โหมดโฟกัส", focusOff: (percent: number) => `ลดเพลงเหลือ ${percent}%`,
    focusOn: (prior: number | null) => prior === null ? "เปิดอยู่" : `เปิดอยู่ · กดกลับไป ${prior}%`,
    reset: "คืนค่าเดิม", resetNote: "คืนระดับเสียงเกมและเพลงก่อนที่เกาะเบาะปรับ",
    muteLabel: (name: string) => `ปิดเสียง ${name}`, mutedValue: "ปิด", close: "ปิดแผง",
    pickTitle: "แอปเพลง", noChoices: "ยังไม่พบแอปที่เล่นเสียง",
    gameMissing: ["ยังไม่พบเสียงของ Dota", "เกมยังไม่ได้เล่นเสียง จึงยังปรับไม่ได้"],
    musicMissing: ["ไม่พบแอปเพลง", "เปิดเพลงในเบราว์เซอร์หรือ Spotify ก่อน"],
    unreadable: ["อ่านระดับเสียงไม่ได้", "Windows ไม่ให้อ่านเสียงของแอปนี้"],
    unavailable: ["ระบบเสียงของ Windows ไม่พร้อม", "ลองใหม่อีกครั้งภายหลัง"],
    kohborMuteUnknown: "ไม่ทราบสถานะปิดเสียง",
  },
  en: {
    game: "Game", gameSub: "Dota 2", kohbor: "Kohbor", kohborSub: "Announcer", music: "Music",
    change: "Change", focus: "Focus mode", focusOff: (percent: number) => `Lower music to ${percent}%`,
    focusOn: (prior: number | null) => prior === null ? "On" : `On · back to ${prior}%`,
    reset: "Reset", resetNote: "Restores game and music levels from before Kohbor changed them",
    muteLabel: (name: string) => `Mute ${name}`, mutedValue: "Off", close: "Close panel",
    pickTitle: "Music app", noChoices: "No app is playing sound",
    gameMissing: ["Dota sound not found", "The game has not played sound yet, so it cannot be set"],
    musicMissing: ["No music app found", "Start music in a browser or Spotify first"],
    unreadable: ["Volume cannot be read", "Windows did not let Kohbor read this app"],
    unavailable: ["Windows audio is not ready", "Try again later"],
    kohborMuteUnknown: "Mute state unknown",
  },
} as const;

const REASONS: Record<string, { th: string; en: string }> = {
  MIXER_SESSION_MISSING: { th: "ไม่พบเสียงของแอปนี้แล้ว", en: "The app's sound session is gone" },
  MIXER_SESSION_UNREADABLE: { th: "อ่านระดับเสียงของแอปนี้ไม่ได้", en: "The app's volume cannot be read" },
  MIXER_SET_FAILED: { th: "Windows ไม่ยอมเปลี่ยนเสียง แสดงค่าเดิมที่อ่านได้", en: "Windows refused the change; showing the last read level" },
  MIXER_DEVICE_CHANGED: { th: "อุปกรณ์เสียงเปลี่ยน อ่านใหม่แล้ว ลองอีกครั้ง", en: "The audio device changed; re-read, try again" },
  MIXER_AUDIO_UNAVAILABLE: { th: "ระบบเสียงของ Windows ไม่พร้อม", en: "Windows audio is not available" },
  MIXER_RECOVERY_WRITE_FAILED: { th: "บันทึกระดับเดิมไม่ได้ จึงไม่ได้เปลี่ยนเสียง", en: "Could not save the original level, so nothing changed" },
  MIXER_RESTORE_PENDING: { th: "บางแอปยังไม่เปิด จะคืนค่าให้เมื่อแอปเล่นเสียงอีกครั้ง", en: "Some apps are closed; they are restored when they play sound again" },
  MIXER_APP_NOT_AVAILABLE: { th: "แอปนี้ไม่ได้เล่นเสียงอยู่", en: "That app is not playing sound" },
  MIXER_TIMEOUT: { th: "ระบบเสียงตอบช้าเกินไป", en: "Windows audio did not answer in time" },
  MIXER_UNAVAILABLE: { th: "มิกเซอร์ไม่พร้อม", en: "The mixer is not available" },
  AUDIO_PREFS_WRITE_FAILED: { th: "บันทึกระดับเสียงพากย์ไม่ได้", en: "Could not save the announcer volume" },
  ORB_PANEL_UNPLACEABLE: { th: "จอเล็กเกินไป แผงจะบังส่วนสำคัญของเกม", en: "The screen is too small; the panel would cover the game HUD" },
};

export function reasonText(code: string | null, locale: OrbLocale): string | null {
  if (!code) return null;
  return REASONS[code]?.[locale] ?? (locale === "th" ? `ทำไม่สำเร็จ (${code})` : `Not done (${code})`);
}

/** The honest note under the faders: missing/unreadable states are explained, never shown as controllable. */
export function channelNote(view: MixerView | null, locale: OrbLocale): readonly [string, string] | null {
  const text = orbCopy[locale];
  if (!view) return text.unavailable;
  if (!view.available) return text.unavailable;
  if (view.game.state === "missing") return text.gameMissing;
  if (view.game.state === "unreadable" || view.music.state === "unreadable") return text.unreadable;
  if (view.music.state === "missing") return text.musicMissing;
  return null;
}

export function valueText(reading: ChannelReading | null, locale: OrbLocale): string {
  if (!controllable(reading)) return "—";
  if (reading?.muted === true) return orbCopy[locale].mutedValue;
  return `${reading?.percent ?? 0}%`;
}

const HIDE_REASONS: Record<string, { th: string; en: string }> = {
  disabled: { th: "ปิดอยู่", en: "Off" },
  not_in_match: { th: "ซ่อนอยู่: ยังไม่อยู่ในแมตช์", en: "Hidden: not in a match" },
  dota_not_foreground: { th: "ซ่อนอยู่: Dota ไม่ได้อยู่หน้าสุด", en: "Hidden: Dota is not in front" },
  exclusive_fullscreen: { th: "ซ่อนอยู่: Dota เป็นเต็มจอแบบ exclusive", en: "Hidden: Dota is in exclusive fullscreen" },
  placement_unavailable: { th: "ซ่อนอยู่: วางตำแหน่งบนจอไม่ได้", en: "Hidden: cannot be placed on the screen" },
  failed_closed: { th: "ซ่อนจนจบแมตช์: ตรวจพบว่าคลิกแล้ว Dota เสียโฟกัส", en: "Hidden until the match ends: a click took focus from Dota" },
};

export function hideReasonText(reason: string | null, locale: OrbLocale): string | null {
  if (!reason) return null;
  return HIDE_REASONS[reason]?.[locale] ?? reason;
}

export interface OrbPrefsView {
  enabled: boolean;
  dwell_ms: number;
  focus_percent: number;
  music_exe: string | null;
  focus_hotkey_available: boolean;
  status: {
    running: boolean;
    phase: string | null;
    reason: string | null;
    failed_closed: boolean;
    interactions: number;
    activations: number;
    start_error: string | null;
    windows_only: boolean;
  };
}

export function parseOrbPrefs(value: unknown): OrbPrefsView | null {
  if (typeof value !== "object" || value === null) return null;
  const v = value as Record<string, unknown>;
  const s = (typeof v.status === "object" && v.status !== null ? v.status : {}) as Record<string, unknown>;
  if (typeof v.enabled !== "boolean" || typeof v.dwell_ms !== "number" || typeof v.focus_percent !== "number") return null;
  return {
    enabled: v.enabled,
    dwell_ms: v.dwell_ms,
    focus_percent: v.focus_percent,
    music_exe: typeof v.music_exe === "string" ? v.music_exe : null,
    focus_hotkey_available: v.focus_hotkey_available === true,
    status: {
      running: s.running === true,
      phase: typeof s.phase === "string" ? s.phase : null,
      reason: typeof s.reason === "string" ? s.reason : null,
      failed_closed: s.failed_closed === true,
      interactions: typeof s.interactions === "number" ? s.interactions : 0,
      activations: typeof s.activations === "number" ? s.activations : 0,
      start_error: typeof s.start_error === "string" ? s.start_error : null,
      windows_only: s.windows_only === true,
    },
  };
}
