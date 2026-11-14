// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { CapturePhase } from "./types.ts";

/** Presentation only: stale capture must keep the same enabled frame subscription. */
export function deriveMinimapPresentation(
  phase: CapturePhase | null,
  reason: string | null,
  hasFrame: boolean,
  locale: "th" | "en",
): { dimmed: boolean; coveredBadge: string | null; staleOverlay: "age" | "waiting" | null } {
  const stale = phase === "stale";
  const covered = stale && reason === "occluded";
  return {
    dimmed: stale,
    coveredBadge: covered ? (locale === "th" ? "ถูกบัง" : "Covered") : null,
    // A mounted preview may not yet have a clean frame. Never claim it has one.
    staleOverlay: stale ? (covered ? (hasFrame ? null : "waiting") : "age") : null,
  };
}
