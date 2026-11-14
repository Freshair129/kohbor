// @ts-ignore Explicit source extensions support the Node test runner.
import { isDemoOutcomeRecord, type AudioHistory, type AudioOutcomeRecord, type HistoryLoadState, type HistoryOutcome } from "../history.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import { eventLabel } from "../eventLabels.ts";
// @ts-ignore Explicit source extensions support the Node test runner.
import type { PlaybackState } from "../playback.ts";

export function relativeHistoryAge(decidedMs: number, hostNowMs: number | null, locale: "th" | "en"): string {
  if (hostNowMs === null || !Number.isFinite(hostNowMs) || !Number.isFinite(decidedMs) || decidedMs < 0 || hostNowMs < decidedMs) return "—";
  return `${Math.floor((hostNowMs - decidedMs) / 1000)} ${locale === "th" ? "วิ" : "s"}`;
}
function outcomeLabel(outcome: HistoryOutcome, locale: "th" | "en"): string {
  const words: Partial<Record<HistoryOutcome, [string, string]>> = {
    admitted: ["เล่นเสียง", "Played"], muted: ["ปิดเสียง", "Muted"], unmapped: ["ไม่มีคลิป", "No clip"],
    cooldown: ["พักก่อน", "Cooldown"], preempted: ["ถูกแทรก", "Preempted"], not_voiced: ["ไม่มีเสียงประกาศ", "Not voiced"],
  };
  return words[outcome]?.[locale === "th" ? 0 : 1] ?? (locale === "th" ? "ไม่เล่น" : "Not played");
}
function newest(history: AudioHistory | null): AudioOutcomeRecord[] {
  return [...(history?.records ?? [])].sort((a, b) => b.decided_monotonic_ms - a.decided_monotonic_ms);
}
function row(record: AudioOutcomeRecord, hostNowMs: number | null, locale: "th" | "en") {
  return { record, eventLabel: eventLabel(record.event_type, locale), audioClass: record.audio_class,
    outcomeLabel: outcomeLabel(record.outcome, locale), outcomeCode: record.outcome,
    age: relativeHistoryAge(record.decided_monotonic_ms, hostNowMs, locale), demo: isDemoOutcomeRecord(record) };
}
export function deriveEventFeed(history: AudioHistory | null, state: HistoryLoadState, hostNowMs: number | null, locale: "th" | "en") {
  const failed = state === "failed" || state === "unreadable";
  const rows = state === "ok" ? newest(history).slice(0, 30).map(record => row(record, hostNowMs, locale)) : [];
  const message = failed ? (locale === "th" ? "อ่านเหตุการณ์ไม่ได้" : "Could not read events")
    : state === "loading" ? (locale === "th" ? "กำลังอ่านเหตุการณ์…" : "Reading events…")
      : rows.length === 0 ? (locale === "th" ? "ยังไม่มีเหตุการณ์" : "No events yet") : null;
  return { rows, message, failed };
}
export function deriveLastClip(history: AudioHistory | null, state: HistoryLoadState, hostNowMs: number | null, locale: "th" | "en") {
  if (state !== "ok") return null;
  const record = newest(history).find(record => record.outcome === "admitted" && record.playback_id !== null);
  return record ? row(record, hostNowMs, locale) : null;
}
export function deriveOnAir(playback: PlaybackState, locale: "th" | "en") {
  const active = playback.audio ?? playback.visual;
  return active ? { playbackId: active.playback_id, eventLabel: eventLabel(active.event_type, locale) } : null;
}
export function shortPlaybackId(id: string): string {
  return id.length <= 16 ? id : `${id.slice(0, 9)}…${id.slice(-5)}`;
}
