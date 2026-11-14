export type MinimapState = "not_running" | "launching" | "hero_selection" | "loading" | "first_frame" | "live" | "occluded" | "unsupported";
export interface SteamStatus {
  steam_running: boolean | null;
  dota_running: boolean | null;
  persona_name: string | null;
  gsi: "receiving" | "waiting" | "unknown";
  game_phase: "none" | "draft" | "pregame" | "live" | "post_game" | "unknown";
  launch_pending: boolean;
  minimap_state: MinimapState;
}
export interface LaunchResult {
  result: "launched" | "steam_not_found" | "already_running" | "blocked_live_match" | "cooldown" | "failed";
  detail: string | null;
}
const mapStates = new Set(["not_running", "launching", "hero_selection", "loading", "first_frame", "live", "occluded", "unsupported"]);
const phases = new Set(["none", "draft", "pregame", "live", "post_game", "unknown"]);
const results = new Set(["launched", "steam_not_found", "already_running", "blocked_live_match", "cooldown", "failed"]);
const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const nullableBoolean = (value: unknown) => value === null || typeof value === "boolean";

export function parseSteamStatus(value: unknown): SteamStatus | null {
  if (!object(value) || !nullableBoolean(value.steam_running) || !nullableBoolean(value.dota_running)
    || !(value.persona_name === null || typeof value.persona_name === "string") || typeof value.launch_pending !== "boolean"
    || !["receiving", "waiting", "unknown"].includes(value.gsi as string)
    || !phases.has(value.game_phase as string) || !mapStates.has(value.minimap_state as string)) return null;
  return { steam_running: value.steam_running as boolean | null, dota_running: value.dota_running as boolean | null,
    persona_name: value.persona_name as string | null, gsi: value.gsi as SteamStatus["gsi"], game_phase: value.game_phase as SteamStatus["game_phase"],
    launch_pending: value.launch_pending, minimap_state: value.minimap_state as MinimapState };
}
export function parseLaunchResult(value: unknown): LaunchResult | null {
  if (!object(value) || !results.has(value.result as string) || !(value.detail === null || typeof value.detail === "string")) return null;
  return { result: value.result as LaunchResult["result"], detail: value.detail };
}
export function steamPersonaName(status: SteamStatus | null, show: boolean): string | null {
  return show && status?.persona_name?.trim() ? status.persona_name : null;
}
export function deriveSteamChips(status: SteamStatus | null, locale: "th" | "en") {
  return {
    steam: status?.steam_running === true ? locale === "th" ? "ออนไลน์" : "Online" : status?.steam_running === false ? locale === "th" ? "ไม่พบ" : "Not found" : "—",
    dota: status?.dota_running === true ? locale === "th" ? "กำลังทำงาน" : "Running" : status?.dota_running === false ? locale === "th" ? "ปิดอยู่" : "Closed" : "—",
    gsi: status?.gsi === "receiving" ? locale === "th" ? "รับข้อมูล" : "Receiving" : status?.gsi === "waiting" ? locale === "th" ? "รอ" : "Waiting" : "—",
  };
}
export function deriveLaunchView(status: SteamStatus | null, busy: boolean, locale: "th" | "en"): { enabled: boolean; reason: string | null } {
  const reason = busy ? (locale === "th" ? "กำลังเปิดอยู่" : "Launch pending")
    : !status || status.dota_running === null ? (locale === "th" ? "ตรวจสถานะไม่ได้" : "Status unknown")
    : status.game_phase === "pregame" || status.game_phase === "live" ? (locale === "th" ? "กำลังอยู่ในแมตช์" : "Match in progress")
    : status.dota_running ? (locale === "th" ? "เกมกำลังทำงาน" : "Game already running")
    : status.launch_pending ? (locale === "th" ? "กำลังเปิดอยู่" : "Launch pending") : null;
  return { enabled: reason === null, reason };
}
export function launchResultText(result: LaunchResult | null, locale: "th" | "en"): string {
  if (!result) return locale === "th" ? "ตรวจผลการเปิดไม่ได้" : "Launch result unknown";
  const words = locale === "th" ? { launched: "ส่งคำสั่งเปิดผ่าน Steam แล้ว", steam_not_found: "ไม่พบ Steam", already_running: "เกมกำลังทำงาน",
    blocked_live_match: "กำลังอยู่ในแมตช์", cooldown: "เพิ่งส่งคำสั่ง — รอสักครู่", failed: "เปิด Dota 2 ไม่สำเร็จ" }
    : { launched: "Launch requested through Steam", steam_not_found: "Steam not found", already_running: "Game already running",
      blocked_live_match: "Match in progress", cooldown: "Recently requested — please wait", failed: "Could not launch Dota 2" };
  return words[result.result] + (result.result === "failed" && result.detail ? ` · ${result.detail}` : "");
}
