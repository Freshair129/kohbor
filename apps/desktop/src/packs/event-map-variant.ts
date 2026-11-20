import type { EventVariant } from "./variant-model";

export type EventMapSchemaVersion = "1.0.0" | "1.1.0";

const requiredKeys = ["variant_id", "duration_ms", "has_visual"];

function validCaption(value: unknown): value is string {
  if (typeof value !== "string" || value.length === 0 || value.length > 400) return false;
  // Use Unicode White_Space for the same trim boundary as Rust str::trim.
  return !/^\p{White_Space}|\p{White_Space}$/u.test(value)
    && !/[\p{Cc}\p{Cs}\u2028\u2029]/u.test(value)
    && Array.from(value).length <= 200;
}

/** Parses one variant from a versioned media_event_map binding. */
export function parseEventMapVariant(value: unknown, version: EventMapSchemaVersion): EventVariant | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const variant = value as Record<string, unknown>;
  const allowedKeys = version === "1.1.0"
    ? [...requiredKeys, "clip_id", "caption"]
    : [...requiredKeys, "clip_id"];
  if (!requiredKeys.every((key) => Object.hasOwn(variant, key))
    || Object.keys(variant).some((key) => !allowedKeys.includes(key))) return null;

  const { variant_id, duration_ms, has_visual, clip_id, caption } = variant;
  if (typeof variant_id !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,79}$/.test(variant_id)
    || typeof duration_ms !== "number" || !Number.isSafeInteger(duration_ms)
    || duration_ms < 0 || duration_ms > 15_000
    || typeof has_visual !== "boolean") return null;
  if (Object.hasOwn(variant, "clip_id")
    && (typeof clip_id !== "string" || !/^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,127}$/.test(clip_id))) return null;
  if (Object.hasOwn(variant, "caption") && !validCaption(caption)) return null;

  return {
    variant_id,
    duration_ms,
    has_visual,
    ...(Object.hasOwn(variant, "clip_id") && typeof clip_id === "string" ? { clip_id } : {}),
    ...(Object.hasOwn(variant, "caption") && typeof caption === "string" ? { caption } : {}),
  };
}
