export interface HostClockAnchor { hostMs: number; localMs: number }
/** A lifecycle event establishes the shared host monotonic clock; wall time is never used. */
export function hostNowFromAnchor(anchor: HostClockAnchor | null, localMs: number): number | null {
  if (!anchor || !Number.isFinite(localMs) || !Number.isFinite(anchor.hostMs) || !Number.isFinite(anchor.localMs)
    || anchor.hostMs < 0 || localMs < anchor.localMs) return null;
  return anchor.hostMs + localMs - anchor.localMs;
}
