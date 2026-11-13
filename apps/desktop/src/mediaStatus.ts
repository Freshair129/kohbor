import type { RuntimeStatus } from "./types";

/**
 * Whether audio is muted, as far as the renderer can honestly tell.
 *
 * The host's readiness check tests mute before anything else, so a reason of
 * AUDIO_MUTED means muted, and a missing reason or any media reason
 * (MEDIA_UNMAPPED, MEDIA_INVALID, MEDIA_PROBE_FAILED) means not muted.
 * CAPABILITY_UNAVAILABLE is different: it is also reported while the audio engine
 * is unresponsive, when mute may be set but cannot be read. Then the last known
 * value (for example from a set_muted acknowledgement) is kept, and `null` means
 * "cannot confirm". The value is never guessed from silence.
 */
export function deriveMuted(media: RuntimeStatus["media"] | null, previous: boolean | null): boolean | null {
  if (media === null) return previous;
  switch (media.reason) {
    case "AUDIO_MUTED":
      return true;
    case null:
    case "MEDIA_UNMAPPED":
    case "MEDIA_INVALID":
    case "MEDIA_PROBE_FAILED":
      return false;
    default:
      return previous;
  }
}

export type MediaTone = "unknown" | "fresh" | "warn" | "off";

export function mediaTone(media: RuntimeStatus["media"] | null): MediaTone {
  if (media === null) return "unknown";
  return media.state === "ready" ? "fresh" : media.state === "not_ready" ? "warn" : "off";
}
