import type { Phase } from "../copy";
import type { RuntimeStatus } from "../types";

export type HeaderTone = "fresh" | "stale" | "unknown" | "off" | "warn";

export type ShellDestination = "overview" | "live" | "packs" | "insights" | "store" | "status" | "logs" | "settings";
export type ShellNavigationGroup = "primary" | "secondary";
export type ShellNavigationItem = { id: ShellDestination; group: ShellNavigationGroup };

export const DEFAULT_SHELL_DESTINATION = "live" satisfies ShellDestination;

export const SHELL_NAV_ITEMS = [
  { id: "overview", group: "primary" },
  { id: "live", group: "primary" },
  { id: "packs", group: "primary" },
  { id: "insights", group: "primary" },
  { id: "store", group: "primary" },
  { id: "status", group: "secondary" },
  { id: "logs", group: "secondary" },
  { id: "settings", group: "secondary" },
] as const satisfies readonly ShellNavigationItem[];

export function navigationItemState(destination: ShellDestination, current: ShellDestination | "account") {
  const active = current === destination;
  const tabStop = active || (current === "account" && destination === DEFAULT_SHELL_DESTINATION);
  return { ariaCurrent: active ? "page" as const : undefined, tabIndex: tabStop ? 0 : -1 };
}

export function nextShellDestination(current: ShellDestination, key: string): ShellDestination | null {
  const destinations = SHELL_NAV_ITEMS.map((item) => item.id);
  const index = destinations.indexOf(current);
  if (index < 0) return null;

  if (key === "Home") return destinations[0] ?? null;
  if (key === "End") return destinations[destinations.length - 1] ?? null;
  if (key === "ArrowDown") return destinations[(index + 1) % destinations.length] ?? null;
  if (key === "ArrowUp") return destinations[(index + destinations.length - 1) % destinations.length] ?? null;
  return null;
}

export type StatusLineLabels = {
  gsi: string;
  gamePhase: string;
  gameAge: string;
  capture: string;
  minimapRecording: string;
  gsiSetup: string;
};

export type StatusLineModelInput = {
  labels: StatusLineLabels;
  gsi: { label: string; tone: HeaderTone };
  phase: { label: string; tone: HeaderTone };
  gameAge: string;
  capture: { label: string; tone: HeaderTone };
  minimapRecording: boolean;
  deck: { label: string; tone: HeaderTone; reason: string };
  setupAvailable: boolean;
  hostLost: boolean;
};

export type StatusLineItem = {
  id: "gsi" | "phase" | "age" | "capture" | "recording" | "deck";
  label: string;
  tone: HeaderTone;
  detail?: string;
};

export function statusLineModel(input: StatusLineModelInput) {
  const { labels } = input;
  const items: StatusLineItem[] = [
    { id: "gsi", label: `${labels.gsi} · ${input.gsi.label}`, tone: input.gsi.tone },
    { id: "phase", label: `${labels.gamePhase}: ${input.phase.label}`, tone: input.phase.tone },
    { id: "age", label: `${labels.gameAge}: ${input.gameAge}`, tone: "unknown" },
    { id: "capture", label: `${labels.capture} · ${input.capture.label}`, tone: input.capture.tone },
    ...(input.minimapRecording
      ? [{ id: "recording" as const, label: labels.minimapRecording, tone: "warn" as const }]
      : []),
    { id: "deck", label: input.deck.label, tone: input.deck.tone, detail: input.deck.reason },
  ];
  return {
    items,
    setupAction: input.setupAvailable ? { label: labels.gsiSetup, destination: "status" as const } : null,
    hostLost: input.hostLost,
  };
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
