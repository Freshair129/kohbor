import type { EventVariantsCopy } from "./event-variants.copy.ts";

export type EventVariantsLocale = "th" | "en";
export type VariantSelection = "random" | "shuffle" | "sequence" | null;

export interface EventVariant {
  variant_id: string;
  clip_id?: string;
  caption?: string;
  duration_ms: number;
  has_visual: boolean;
}

export interface VariantAuditionResult {
  audio_started: boolean;
  audio_ended: boolean;
  audio_end_reason: string | null;
  visual_status: "shown" | "failed" | "none" | "unknown";
  visual_reason: string | null;
  video_error_code: number | null;
}

export interface VariantRowPresentation {
  captionText: string;
  durationText: string;
  bannerText: string;
}

export function browseVoiceLabel(variant: EventVariant, eventName: string, index: number, locale: EventVariantsLocale): string {
  const fallback = locale === "th" ? `เสียงที่ ${index + 1}` : `Voice ${index + 1}`;
  return variant.caption ?? `${eventName} · ${fallback}`;
}

export function variantSelectionLabel(
  selection: VariantSelection,
  copy: EventVariantsCopy,
): string | null {
  if (selection === null) return null;
  return copy.selection[selection];
}

export function variantRowPresentation(
  variant: EventVariant,
  copy: EventVariantsCopy,
): VariantRowPresentation {
  return {
    captionText: variant.caption ?? copy.captionMissing,
    durationText: `${(variant.duration_ms / 1_000).toFixed(1)} ${copy.secondsUnit}`,
    bannerText: variant.has_visual ? copy.bannerYes : copy.bannerNo,
  };
}

export function auditionOutcomeText(
  result: VariantAuditionResult,
  copy: EventVariantsCopy,
): string {
  const audio = !result.audio_started
    ? copy.audioNotStarted
    : result.audio_ended
      ? copy.audioEnded
      : copy.audioStarted;
  const visual = result.visual_status === "shown"
    ? copy.visualShown
    : result.visual_status === "failed"
      ? copy.visualFailed
      : result.visual_status === "none"
        ? copy.visualNone
        : copy.visualUnknown;
  return `${audio} · ${visual}`;
}

export function auditionFailureText(error: unknown, copy: EventVariantsCopy): string {
  const code = typeof error === "string" ? error : "";
  return copy.auditionFailure[code] ?? copy.auditionFailed;
}
