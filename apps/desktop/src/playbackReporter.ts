export type VideoReason = "NONE" | "VIDEO_ERROR" | "VIDEO_URL_UNAVAILABLE" | "VIDEO_START_TIMEOUT";

const REPORTED_LIMIT = 64;

/** Remembers which playbacks already reported, per playback ID, so a late timer for an earlier playback can never
 * report a timeout after that playback started (the live log showed 8 false VIDEO_START_TIMEOUT lines). */
export function createPlaybackReporter(emit: (playbackId: string, outcome: "started" | "failed", errorCode: number | null, reasonCode: VideoReason) => void) {
  const reported = new Set<string>();
  return (playbackId: string | null, outcome: "started" | "failed", errorCode: number | null, reasonCode: VideoReason): boolean => {
    if (!playbackId || reported.has(playbackId)) return false;
    reported.add(playbackId);
    if (reported.size > REPORTED_LIMIT) reported.delete(reported.values().next().value as string);
    emit(playbackId, outcome, errorCode, reasonCode);
    return true;
  };
}
