import type { EventVariant } from "./variant-model";

export type EventMapSchemaVersion = "1.0.0" | "1.1.0" | "1.2.0" | "1.3.0";

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
  const current = version === "1.2.0" || version === "1.3.0";
  const allowedKeys = version === "1.3.0"
    ? [...requiredKeys, "clip_id", "caption", "install_id", "package_id", "equipped_slot", "state", "reason", "hero"]
    : version === "1.2.0"
      ? [...requiredKeys, "clip_id", "caption", "install_id", "package_id", "equipped_slot", "state", "reason"]
    : version === "1.1.0" ? [...requiredKeys, "clip_id", "caption"] : [...requiredKeys, "clip_id"];
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
  if (current) {
    const sourceKeys = ["install_id", "package_id", "equipped_slot"];
    if (sourceKeys.some(key => Object.hasOwn(variant, key)) && !sourceKeys.every(key => Object.hasOwn(variant, key))) return null;
    if (Object.hasOwn(variant, "install_id") && (typeof variant.install_id !== "string" || !/^ins_[0-9]{10}$/u.test(variant.install_id))) return null;
    if (Object.hasOwn(variant, "package_id") && (typeof variant.package_id !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/u.test(variant.package_id))) return null;
    if (Object.hasOwn(variant, "equipped_slot") && (typeof variant.equipped_slot !== "number" || !Number.isInteger(variant.equipped_slot) || variant.equipped_slot < 0 || variant.equipped_slot > 3)) return null;
    if (version === "1.3.0" && Object.hasOwn(variant, "hero")
      && (typeof variant.hero !== "string" || !/^npc_dota_hero_[a-z0-9_]{1,50}$/.test(variant.hero))) return null;
    if (variant.state !== "ready" && variant.state !== "not_loaded" && variant.state !== "invalid_media") return null;
    if (typeof variant.reason !== "string" || !/^[A-Z][A-Z0-9_]{1,63}$/.test(variant.reason)) return null;
  }

  return {
    variant_id,
    duration_ms,
    has_visual,
    ...(Object.hasOwn(variant, "clip_id") && typeof clip_id === "string" ? { clip_id } : {}),
    ...(Object.hasOwn(variant, "caption") && typeof caption === "string" ? { caption } : {}),
    ...(version === "1.3.0" && typeof variant.hero === "string" ? { hero: variant.hero } : {}),
    ...(typeof variant.install_id === "string" ? { install_id: variant.install_id } : {}),
    ...(typeof variant.package_id === "string" ? { package_id: variant.package_id } : {}),
    ...(typeof variant.equipped_slot === "number" ? { equipped_slot: variant.equipped_slot } : {}),
    ...(current ? { state: variant.state as EventVariant["state"], reason: variant.reason as string } : {}),
  };
}
