export const DEFAULT_MINIMAP_RECORDER_CONFIG = Object.freeze({
  enabled: false,
  sampling_rate_fps: 2,
  total_cap_bytes: 5_000_000_000,
});

export interface MinimapRecorderMatch {
  match_hash: string;
  frame_count: number;
  frame_bytes: number;
  total_bytes: number;
  ally_hero_ids: string[] | null;
  enemy_hero_ids: string[] | null;
}

export interface MinimapRecorderStatus {
  available: true;
  enabled: boolean;
  sampling_rate_fps: number;
  total_cap_bytes: number;
  per_match_frame_cap_bytes: number;
  recording: boolean;
  recordings_count: number;
  total_bytes: number;
  dropped_frames: number;
  stop_reason: string | null;
  matches: MinimapRecorderMatch[];
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function natural(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function heroIds(value: unknown): string[] | null | undefined {
  if (value === null) return null;
  if (!Array.isArray(value) || !value.every((id) => typeof id === "string" && /^npc_dota_hero_[a-z0-9_]+$/.test(id))) return undefined;
  return [...value] as string[];
}

function parseMatch(value: unknown): MinimapRecorderMatch | null {
  if (!isObject(value)
    || typeof value.match_hash !== "string"
    || !/^[a-f0-9]{64}$/.test(value.match_hash)
    || !natural(value.frame_count)
    || !natural(value.frame_bytes)
    || !natural(value.total_bytes)) return null;
  const allies = heroIds(value.ally_hero_ids);
  const enemies = heroIds(value.enemy_hero_ids);
  if (allies === undefined || enemies === undefined) return null;
  return {
    match_hash: value.match_hash,
    frame_count: value.frame_count,
    frame_bytes: value.frame_bytes,
    total_bytes: value.total_bytes,
    ally_hero_ids: allies,
    enemy_hero_ids: enemies,
  };
}

export function parseMinimapRecorderStatus(value: unknown): MinimapRecorderStatus | null {
  if (!isObject(value)
    || value.available !== true
    || typeof value.enabled !== "boolean"
    || !Number.isInteger(value.sampling_rate_fps)
    || Number(value.sampling_rate_fps) < 1
    || Number(value.sampling_rate_fps) > 5
    || !natural(value.total_cap_bytes)
    || value.total_cap_bytes === 0
    || !natural(value.per_match_frame_cap_bytes)
    || typeof value.recording !== "boolean"
    || !natural(value.recordings_count)
    || !natural(value.total_bytes)
    || !natural(value.dropped_frames)
    || (value.stop_reason !== null && typeof value.stop_reason !== "string")
    || !Array.isArray(value.matches)) return null;
  const matches = value.matches.map(parseMatch);
  if (matches.some((match) => match === null) || value.recordings_count !== matches.length) return null;
  return {
    available: true,
    enabled: value.enabled,
    sampling_rate_fps: Number(value.sampling_rate_fps),
    total_cap_bytes: value.total_cap_bytes,
    per_match_frame_cap_bytes: value.per_match_frame_cap_bytes,
    recording: value.recording,
    recordings_count: value.recordings_count,
    total_bytes: value.total_bytes,
    dropped_frames: value.dropped_frames,
    stop_reason: value.stop_reason,
    matches: matches as MinimapRecorderMatch[],
  };
}

export function recordingIndicatorState(status: MinimapRecorderStatus | null): "recording" | "enabled" | null {
  if (status?.available !== true || !status.enabled) return null;
  return status.recording ? "recording" : "enabled";
}

export function shouldShowRecordingIndicator(status: MinimapRecorderStatus | null): boolean {
  return recordingIndicatorState(status) === "recording";
}

export function formatRecordingBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "kB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return unit === 0 ? `${Math.floor(value)} ${units[unit]}` : `${value.toFixed(1)} ${units[unit]}`;
}
