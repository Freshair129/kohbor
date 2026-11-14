import type { Phase } from "../copy";
import type { RuntimeStatus } from "../types";

export type ShellTab = "live" | "packs" | "settings" | "status";
export type HeaderTone = "fresh" | "stale" | "unknown" | "off" | "warn";

export const SHELL_TABS: readonly ShellTab[] = ["live", "packs", "settings", "status"];

export function nextShellTab(current: ShellTab, key: string): ShellTab | null {
  const index = SHELL_TABS.indexOf(current);
  if (index < 0) return null;

  if (key === "Home") return SHELL_TABS[0] ?? null;
  if (key === "End") return SHELL_TABS[SHELL_TABS.length - 1] ?? null;
  if (key === "ArrowRight") return SHELL_TABS[(index + 1) % SHELL_TABS.length] ?? null;
  if (key === "ArrowLeft") return SHELL_TABS[(index + SHELL_TABS.length - 1) % SHELL_TABS.length] ?? null;
  return null;
}

export type HeaderStatus = Pick<RuntimeStatus, "session" | "intake">;
export type CaptureHeaderInput = { phase: string; paused: boolean | null; phase_reason?: string | null } | null;
export interface HeaderLabels {
  phases: Record<Phase, string>;
  staleSuffix: string;
  capturePhases: Record<string, string>;
  capturePaused: string;
  captureRunning: string;
  captureCovered?: string;
}

export function deriveHeaderState(
  status: HeaderStatus | null,
  capture: CaptureHeaderInput,
  hostLost: boolean,
  labels?: HeaderLabels,
  formatAge?: (ageMs: number) => string,
) {
  const phase = status?.session.phase;
  const sourceLive = status?.intake.state === "receiving" && !hostLost;
  const readinessTone: HeaderTone = !phase || phase === "none"
    ? "unknown"
    : phase === "unsupported"
      ? "off"
      : sourceLive
        ? "fresh"
        : "stale";
  const readinessLabel = phase === undefined
    ? "—"
    : phase === "none" || phase === "unsupported" || sourceLive
      ? labels?.phases[phase] ?? "—"
      : `${labels?.phases[phase] ?? "—"} (${labels?.staleSuffix ?? "—"})`;

  const ageMs = status?.intake.last_admitted_age_ms;
  const gameAge = hostLost || ageMs === null || ageMs === undefined || !Number.isFinite(ageMs) || ageMs < 0
    ? "—"
    : formatAge?.(ageMs) ?? "—";

  const capturePhase = capture === null
    ? "—"
    : labels?.capturePhases[capture.phase] ?? "—";
  const covered = capture?.phase === "stale" && capture.phase_reason === "occluded";
  const captureLabel = capture === null || capturePhase === "—"
    ? "—"
    : covered ? labels?.captureCovered ?? "—"
    : capture.phase === "idle" ? capturePhase
    : `${capturePhase} · ${capture.paused === null ? "—" : capture.paused ? labels?.capturePaused ?? "—" : labels?.captureRunning ?? "—"}`;
  const captureTone: HeaderTone = capture === null || capturePhase === "—" || capture.phase === "unknown" || capture.phase === "idle"
    ? "unknown"
    : covered ? "stale"
    : capture.paused === true || capture.phase === "unsupported"
      ? "off"
      : capture.phase === "live" && capture.paused === false
        ? "fresh"
        : capture.phase === "lost" || capture.phase === "coverage_unavailable"
          ? "warn"
          : capture.phase === "stale"
            ? "stale"
            : "unknown";

  return {
    readiness: { label: readinessLabel, tone: readinessTone },
    gameAge,
    capture: { label: captureLabel, tone: captureTone },
  };
}
