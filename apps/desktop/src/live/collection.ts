// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { addCaptureCostSample, addResourceSample, emptyResourceSummary, parseCaptureStatus, parseResourceMetrics, parseRiskState } from "./state.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import type { CaptureStatus, ResourceMetrics, ResourceSummary, RiskState, VisionEvent } from "./types.ts";
// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { deriveHostCounterRates, type HostCounterRates } from "./previewDiagnostics.ts";

export type LiveCollectionEventKind = "capture" | "risk" | "resource";
export type TimedValue<T> = { value: T | null; receivedAt: number | null };
export type LiveCollectionSource = (
  kind: LiveCollectionEventKind,
  onPayload: (payload: unknown) => void,
) => void | (() => void) | Promise<void | (() => void)>;

export interface LiveCollectionSnapshot {
  capture: TimedValue<CaptureStatus>;
  captureCounterRates: TimedValue<HostCounterRates>;
  risk: TimedValue<RiskState>;
  lastVisionEvent: TimedValue<VisionEvent>;
  resources: TimedValue<ResourceMetrics>;
  resourceSummary: ResourceSummary;
}

export interface LiveCollection {
  start(source: LiveCollectionSource): Promise<void>;
  subscribe(onChange: () => void): () => void;
  getSnapshot(): LiveCollectionSnapshot;
  resetResourceSummary(): void;
}

const EVENT_KINDS: LiveCollectionEventKind[] = ["capture", "risk", "resource"];

function unknownValue<T>(): TimedValue<T> {
  return { value: null, receivedAt: null };
}

export function createLiveCollection(now: () => number = () => performance.now()): LiveCollection {
  let snapshot: LiveCollectionSnapshot = {
    capture: unknownValue(),
    captureCounterRates: unknownValue(),
    risk: unknownValue(),
    lastVisionEvent: unknownValue(),
    resources: unknownValue(),
    resourceSummary: emptyResourceSummary(),
  };
  let startPromise: Promise<void> | null = null;
  const subscribers = new Set<() => void>();

  function notify() {
    for (const subscriber of subscribers) subscriber();
  }

  function update(kind: LiveCollectionEventKind, payload: unknown) {
    const receivedAt = now();
    if (kind === "capture") {
      const value = parseCaptureStatus(payload);
      const current = { value, receivedAt };
      snapshot = {
        ...snapshot,
        capture: current,
        captureCounterRates: { value: deriveHostCounterRates(snapshot.capture, current), receivedAt },
        resourceSummary: value ? addCaptureCostSample(snapshot.resourceSummary, value) : snapshot.resourceSummary,
      };
    } else if (kind === "risk") {
      const value = parseRiskState(payload);
      const event = value?.events.filter((candidate): candidate is VisionEvent => candidate !== "risk_unavailable").at(-1) ?? null;
      snapshot = {
        ...snapshot,
        risk: { value, receivedAt },
        lastVisionEvent: event ? { value: event, receivedAt } : snapshot.lastVisionEvent,
      };
    } else {
      const value = parseResourceMetrics(payload);
      snapshot = {
        ...snapshot,
        resources: { value, receivedAt },
        resourceSummary: value ? addResourceSample(snapshot.resourceSummary, value) : snapshot.resourceSummary,
      };
    }
    notify();
  }

  return {
    start(source) {
      if (startPromise) return startPromise;
      startPromise = Promise.all(EVENT_KINDS.map(async (kind) => {
        try {
          await source(kind, (payload) => update(kind, payload));
        } catch {
          // A failed listener keeps that event's state unknown.
        }
      })).then(() => undefined);
      return startPromise;
    },
    subscribe(onChange) {
      subscribers.add(onChange);
      return () => subscribers.delete(onChange);
    },
    getSnapshot() {
      return snapshot;
    },
    resetResourceSummary() {
      snapshot = { ...snapshot, resourceSummary: emptyResourceSummary() };
      notify();
    },
  };
}

// App startup owns this singleton; collection begins before any Live panel mounts.
export const liveCollection = createLiveCollection();
