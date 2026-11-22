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

export type ChannelKind = "game" | "kohbor" | "music" | "app";
/** `game`, `kohbor`, `music` or `app:<exe>`. */
export type ChannelId = string;
export type SessionState = "present" | "mixed" | "missing" | "unreadable";

export interface ChannelReading {
  id: ChannelId;
  kind: ChannelKind;
  state: SessionState;
  exe: string | null;
  percent: number | null;
  muted: boolean | null;
  /** Lowest and highest session level when an app's sessions differ (`mixed`). */
  range: readonly [number, number] | null;
}

export type SlotSpec = { kind: "kohbor" } | { kind: "music"; exe: string | null } | { kind: "app"; exe: string };

export interface MixerView {
  available: boolean;
  reason: string | null;
  /** Game first, then the slots in order: one to four channels. */
  channels: ChannelReading[];
  slots: SlotSpec[];
  app_choices: string[];
  music_selected: string | null;
  focus: { active: boolean; percent: number; channels: { id: ChannelId; prior_percent: number | null }[] };
  restore_pending: number;
}

export interface MixerReply { ok: boolean; code: string | null; mixer: MixerView }

const PHASES: readonly OrbPhase[] = ["hidden", "tucked", "charging", "sliding", "out", "open"];
const KINDS: readonly ChannelKind[] = ["game", "kohbor", "music", "app"];
export const WHEEL_STEP = 5;
export const FOCUS_CHOICES = [10, 15, 20, 25, 30, 40, 50] as const;
export const DWELL_CHOICES_MS = [300, 600, 1000] as const;
/** The panel shows at most four channels: Game plus three slots (owner request 2026-11-22). */
export const MAX_CHANNELS = 4;
export const MAX_SLOTS = MAX_CHANNELS - 1;
/** Panel CSS width before the host zoom, by channel count; columns keep the approved three-channel width. */
export const PANEL_CSS_WIDTHS = [154, 154, 220, 288] as const;

export function panelCssWidth(channels: number): number {
  const index = Math.min(MAX_CHANNELS, Math.max(1, Math.round(channels))) - 1;
  return PANEL_CSS_WIDTHS[index] ?? PANEL_CSS_WIDTHS[2];
}

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function finite(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function percentValue(value: unknown): number | null {
  return finite(value) && value >= 0 && value <= 100 ? Math.round(value) : null;
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
/** Owner decision 2026-11-23: only the primary (left) mouse button interacts with the orb; right-click is Dota's move. */
export function isOrbPrimaryButton(button: number): boolean {
  return button === 0;
}

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

function validChannelId(id: string, kind: ChannelKind): boolean {
  if (kind === "app") return /^app:[a-z0-9._ -]{1,60}\.exe$/.test(id);
  return id === kind;
}

function channel(value: unknown): ChannelReading | null {
  const c = record(value);
  if (!c || typeof c.id !== "string" || !KINDS.includes(c.kind as ChannelKind)) return null;
  const kind = c.kind as ChannelKind;
  if (!validChannelId(c.id, kind)) return null;
  const state = c.state;
  if (state !== "present" && state !== "mixed" && state !== "missing" && state !== "unreadable") return null;
  const percent = percentValue(c.percent);
  // A present channel must carry a real level, a mixed one a real range; otherwise it is not controllable.
  if (state === "present" && percent === null) return null;
  let range: readonly [number, number] | null = null;
  if (state === "mixed") {
    const low = Array.isArray(c.range) ? percentValue(c.range[0]) : null;
    const high = Array.isArray(c.range) ? percentValue(c.range[1]) : null;
    if (low === null || high === null || low > high) return null;
    range = [low, high];
  }
  const live = state === "present" || state === "mixed";
  return {
    id: c.id,
    kind,
    state,
    exe: typeof c.exe === "string" ? c.exe : null,
    percent: state === "present" ? percent : null,
    muted: live && typeof c.muted === "boolean" ? c.muted : null,
    range,
  };
}

export function parseSlot(value: unknown): SlotSpec | null {
  const s = record(value);
  if (!s) return null;
  if (s.kind === "kohbor") return { kind: "kohbor" };
  if (s.kind === "music") return { kind: "music", exe: typeof s.exe === "string" ? s.exe : null };
  if (s.kind === "app" && typeof s.exe === "string") return { kind: "app", exe: s.exe };
  return null;
}

function slots(value: unknown): SlotSpec[] {
  if (!Array.isArray(value)) return [];
  const parsed = value.map(parseSlot);
  return parsed.every((slot): slot is SlotSpec => slot !== null) ? parsed.slice(0, MAX_SLOTS) : [];
}

export function parseMixer(value: unknown): MixerView | null {
  const m = record(value);
  if (!m || typeof m.available !== "boolean" || !Array.isArray(m.channels)) return null;
  const channels = m.channels.map(channel);
  if (channels.length < 1 || channels.length > MAX_CHANNELS || channels.some((c) => c === null)) return null;
  const parsed = channels as ChannelReading[];
  // Game is always first and every channel appears once.
  if (parsed[0]?.kind !== "game" || new Set(parsed.map((c) => c.id)).size !== parsed.length) return null;
  if (parsed.slice(1).some((c) => c.kind === "game")) return null;
  const focus = record(m.focus);
  const focusChannels = Array.isArray(focus?.channels)
    ? focus.channels.map(record).filter((entry): entry is Record<string, unknown> => entry !== null && typeof entry.id === "string")
      .map((entry) => ({ id: entry.id as string, prior_percent: percentValue(entry.prior_percent) }))
    : [];
  const choices = Array.isArray(m.app_choices) ? m.app_choices.filter((exe): exe is string => typeof exe === "string") : [];
  return {
    available: m.available,
    reason: typeof m.reason === "string" ? m.reason : null,
    channels: parsed,
    slots: slots(m.slots),
    app_choices: choices,
    music_selected: typeof m.music_selected === "string" ? m.music_selected : null,
    focus: {
      active: focus?.active === true,
      percent: finite(focus?.percent) ? focus.percent as number : 20,
      channels: focusChannels,
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

export function channelById(view: MixerView | null | undefined, id: ChannelId): ChannelReading | null {
  return view?.channels.find((reading) => reading.id === id) ?? null;
}

export function controllable(reading: ChannelReading | null | undefined): boolean {
  if (reading?.state === "present") return reading.percent !== null;
  return reading?.state === "mixed" && reading.range !== null;
}

/** Focus mode lowers every non-game app channel (Music and added apps); the Kohbor announcer keeps critical alerts. */
export function focusTargets(view: MixerView | null | undefined): ChannelReading[] {
  return view?.channels.filter((reading) => (reading.kind === "music" || reading.kind === "app") && controllable(reading)) ?? [];
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
    game: "เกม", gameSub: "Dota 2", kohbor: "เกาะเบาะ", kohborSub: "เสียงพากย์", music: "เพลง", appSub: "แอป",
    change: "เปลี่ยน", focus: "โหมดโฟกัส", focusOff: (percent: number) => `ลดเสียงอื่นเหลือ ${percent}%`,
    focusOn: (prior: number | null) => prior === null ? "เปิดอยู่ · กดอีกครั้งเพื่อคืนค่า" : `เปิดอยู่ · กดกลับไป ${prior}%`,
    reset: "คืนค่าเดิม", resetNote: "คืนระดับเสียงแอปก่อนที่เกาะเบาะปรับ",
    muteLabel: (name: string) => `ปิดเสียง ${name}`, mutedValue: "ปิด", close: "ปิดแผง",
    pickTitle: "แอปเพลง", noChoices: "ยังไม่พบแอปที่เล่นเสียง",
    gameMissing: ["ยังไม่พบเสียงของ Dota", "เกมยังไม่ได้เล่นเสียง จึงยังปรับไม่ได้"],
    musicMissing: ["ไม่พบแอปเพลง", "เปิดเพลงในเบราว์เซอร์หรือ Spotify ก่อน"],
    appMissing: (name: string) => [`ยังไม่พบเสียงของ ${name}`, "เปิดแอปให้เล่นเสียงก่อน จึงจะปรับได้"] as const,
    unreadable: ["อ่านระดับเสียงไม่ได้", "Windows ไม่ให้อ่านเสียงของแอปนี้"],
    unavailable: ["ระบบเสียงของ Windows ไม่พร้อม", "ลองใหม่อีกครั้งภายหลัง"],
    mixedLabel: (name: string, low: number, high: number) => `${name} มีหลายเสียงที่ระดับต่างกัน ${low}–${high}%`,
    kohborMuteUnknown: "ไม่ทราบสถานะปิดเสียง",
  },
  en: {
    game: "Game", gameSub: "Dota 2", kohbor: "Kohbor", kohborSub: "Announcer", music: "Music", appSub: "App",
    change: "Change", focus: "Focus mode", focusOff: (percent: number) => `Lower other sound to ${percent}%`,
    focusOn: (prior: number | null) => prior === null ? "On · press again to restore" : `On · back to ${prior}%`,
    reset: "Reset", resetNote: "Restores app levels from before Kohbor changed them",
    muteLabel: (name: string) => `Mute ${name}`, mutedValue: "Off", close: "Close panel",
    pickTitle: "Music app", noChoices: "No app is playing sound",
    gameMissing: ["Dota sound not found", "The game has not played sound yet, so it cannot be set"],
    musicMissing: ["No music app found", "Start music in a browser or Spotify first"],
    appMissing: (name: string) => [`${name} sound not found`, "Let the app play sound first, then it can be set"] as const,
    unreadable: ["Volume cannot be read", "Windows did not let Kohbor read this app"],
    unavailable: ["Windows audio is not ready", "Try again later"],
    mixedLabel: (name: string, low: number, high: number) => `${name} has several sounds at different levels, ${low}–${high}%`,
    kohborMuteUnknown: "Mute state unknown",
  },
} as const;

/** Column title and subtitle for a channel. */
export function channelLabel(reading: ChannelReading, locale: OrbLocale): { name: string; sub: string } {
  const text = orbCopy[locale];
  switch (reading.kind) {
    case "game": return { name: text.game, sub: text.gameSub };
    case "kohbor": return { name: text.kohbor, sub: text.kohborSub };
    case "music": return { name: text.music, sub: appLabel(reading.exe) };
    case "app": return { name: appLabel(reading.exe), sub: text.appSub };
  }
}

const REASONS: Record<string, { th: string; en: string }> = {
  MIXER_SESSION_MISSING: { th: "ไม่พบเสียงของแอปนี้แล้ว", en: "The app's sound session is gone" },
  MIXER_SESSION_UNREADABLE: { th: "อ่านระดับเสียงของแอปนี้ไม่ได้", en: "The app's volume cannot be read" },
  MIXER_SET_FAILED: { th: "Windows ไม่ยอมเปลี่ยนเสียง แสดงค่าเดิมที่อ่านได้", en: "Windows refused the change; showing the last read level" },
  MIXER_DEVICE_CHANGED: { th: "อุปกรณ์เสียงเปลี่ยน อ่านใหม่แล้ว ลองอีกครั้ง", en: "The audio device changed; re-read, try again" },
  MIXER_AUDIO_UNAVAILABLE: { th: "ระบบเสียงของ Windows ไม่พร้อม", en: "Windows audio is not available" },
  MIXER_RECOVERY_WRITE_FAILED: { th: "บันทึกระดับเดิมไม่ได้ จึงไม่ได้เปลี่ยนเสียง", en: "Could not save the original level, so nothing changed" },
  MIXER_RESTORE_PENDING: { th: "บางแอปยังไม่เปิด จะคืนค่าให้เมื่อแอปเล่นเสียงอีกครั้ง", en: "Some apps are closed; they are restored when they play sound again" },
  MIXER_APP_NOT_AVAILABLE: { th: "แอปนี้ไม่ได้เล่นเสียงอยู่ หรือมีแถวของตัวเองแล้ว", en: "That app is not playing sound, or already has its own row" },
  MIXER_CHANNEL_UNKNOWN: { th: "แถวนี้ไม่ได้ตั้งไว้แล้ว", en: "That row is no longer set up" },
  MIXER_TIMEOUT: { th: "ระบบเสียงตอบช้าเกินไป", en: "Windows audio did not answer in time" },
  MIXER_UNAVAILABLE: { th: "มิกเซอร์ไม่พร้อม", en: "The mixer is not available" },
  AUDIO_PREFS_WRITE_FAILED: { th: "บันทึกระดับเสียงพากย์ไม่ได้", en: "Could not save the announcer volume" },
  ORB_PANEL_UNPLACEABLE: { th: "จอเล็กเกินไป แผงจะบังส่วนสำคัญของเกม", en: "The screen is too small; the panel would cover the game HUD" },
  ORB_SLOTS_INVALID: { th: "ตั้งแถวเสียงไม่ได้ (ซ้ำหรือเกิน 4 แถว)", en: "Those sound rows are not allowed (duplicate or more than 4)" },
};

export function reasonText(code: string | null, locale: OrbLocale): string | null {
  if (!code) return null;
  return REASONS[code]?.[locale] ?? (locale === "th" ? `ทำไม่สำเร็จ (${code})` : `Not done (${code})`);
}

/** The honest note under the faders: missing/unreadable states are explained, never shown as controllable. */
export function channelNote(view: MixerView | null, locale: OrbLocale): readonly [string, string] | null {
  const text = orbCopy[locale];
  if (!view || !view.available) return text.unavailable;
  const game = view.channels[0];
  if (game?.state === "missing") return text.gameMissing;
  if (view.channels.some((reading) => reading.state === "unreadable")) return text.unreadable;
  const missing = view.channels.find((reading) => reading.state === "missing");
  if (missing?.kind === "music") return text.musicMissing;
  if (missing?.kind === "app") return text.appMissing(appLabel(missing.exe));
  return null;
}

export function valueText(reading: ChannelReading | null, locale: OrbLocale): string {
  if (!reading || !controllable(reading)) return "—";
  if (reading.muted === true) return orbCopy[locale].mutedValue;
  if (reading.state === "mixed" && reading.range) return `${reading.range[0]}–${reading.range[1]}%`;
  return `${reading.percent ?? 0}%`;
}

// ---------------------------------------------------------------------------------------------------------------
// Slot editing (Settings)

/** Stable key of a slot choice: `kohbor`, `music` or `app:<exe>`; `""` is an empty row. */
export function slotKey(slot: SlotSpec | null | undefined): string {
  if (!slot) return "";
  return slot.kind === "app" ? `app:${slot.exe}` : slot.kind;
}

/** Choices for row `index`: empty, Kohbor, Music, and every app with a session (or already chosen), minus the
 *  choices other rows hold and the Music app. */
export function slotChoices(slots: readonly SlotSpec[], index: number, appChoices: readonly string[]): string[] {
  const taken = new Set(slots.filter((_, i) => i !== index).map(slotKey));
  const musicExe = slots.find((slot): slot is { kind: "music"; exe: string | null } => slot.kind === "music")?.exe ?? null;
  const apps = [...new Set([...appChoices, ...slots.flatMap((slot) => slot.kind === "app" ? [slot.exe] : [])])]
    .filter((exe) => exe !== "dota2.exe" && exe !== musicExe)
    .sort()
    .map((exe) => `app:${exe}`);
  return ["", "kohbor", "music", ...apps].filter((key) => key === "" || !taken.has(key));
}

/** Put `key` in row `index` (an empty key removes the row). Keeps the Music app choice when Music moves. */
export function setSlot(slots: readonly SlotSpec[], index: number, key: string): SlotSpec[] {
  const next = [...slots];
  const previousMusic = slots.find((slot): slot is { kind: "music"; exe: string | null } => slot.kind === "music");
  let slot: SlotSpec | null = null;
  if (key === "kohbor") slot = { kind: "kohbor" };
  else if (key === "music") slot = { kind: "music", exe: previousMusic?.exe ?? null };
  else if (key.startsWith("app:")) slot = { kind: "app", exe: key.slice(4) };
  if (index >= next.length) {
    if (slot && next.length < MAX_SLOTS) next.push(slot);
    return next;
  }
  if (slot) next[index] = slot;
  else next.splice(index, 1);
  return next;
}

export function moveSlot(slots: readonly SlotSpec[], index: number, delta: -1 | 1): SlotSpec[] {
  const target = index + delta;
  if (index < 0 || index >= slots.length || target < 0 || target >= slots.length) return [...slots];
  const next = [...slots];
  const a = next[index];
  const b = next[target];
  if (!a || !b) return next;
  next[index] = b;
  next[target] = a;
  return next;
}

export function slotText(key: string, locale: OrbLocale): string {
  const text = orbCopy[locale];
  if (key === "") return locale === "th" ? "— ว่าง —" : "— empty —";
  if (key === "kohbor") return `${text.kohbor} (${text.kohborSub})`;
  if (key === "music") return locale === "th" ? `${text.music} (แอปเพลงที่เลือก)` : `${text.music} (chosen music app)`;
  return appLabel(key.slice(4));
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
  slots: SlotSpec[];
  max_slots: number;
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
  if (!Array.isArray(v.slots)) return null;
  const parsedSlots = v.slots.map(parseSlot);
  if (parsedSlots.some((slot) => slot === null) || parsedSlots.length > MAX_SLOTS) return null;
  return {
    enabled: v.enabled,
    dwell_ms: v.dwell_ms,
    focus_percent: v.focus_percent,
    slots: parsedSlots as SlotSpec[],
    max_slots: typeof v.max_slots === "number" ? Math.min(MAX_SLOTS, v.max_slots) : MAX_SLOTS,
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
