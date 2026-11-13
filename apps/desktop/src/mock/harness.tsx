import { useEffect, useState } from "react";
import ownSnapshotFixtureRaw from "../../../../fixtures/own-fresh-at-boundary.json?raw";
import runtimeStatusFixtureRaw from "../../../../fixtures/runtime-status-live-valid.json?raw";
import settingsFixtureRaw from "../../../../fixtures/user-settings-valid.json?raw";
import { copy, type Locale } from "../copy";
import type { PlaybackClass, PlaybackEndReason, PlaybackEvent } from "../playback";
import type { OwnSnapshot, RuntimeStatus, UserSettingsView } from "../types";

type MockCallback = (event: unknown) => void;
type MockEventBinding = { event: string; callbackId: number };
type MockTauriInternals = {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
  transformCallback: (callback: MockCallback) => number;
};

const EVENT_NAME = "playback://event";
const PROBE_EVENT_NAME = "media://probe-request";
const MOCK_CHANNEL = "kohbor-mock-playback";
const MOCK_PROBE_REPORT_EVENT = "kohbor-mock-media-probe-report";
const statusFixture = JSON.parse(runtimeStatusFixtureRaw) as RuntimeStatus;
const snapshotFixture = JSON.parse(ownSnapshotFixtureRaw) as OwnSnapshot;
const settingsFixture = JSON.parse(settingsFixtureRaw) as { revision: number; locale: Locale };
let settingsView: UserSettingsView = {
  revision: settingsFixture.revision,
  locale: settingsFixture.locale as Locale,
};
let muted = false;
let statusRevision = statusFixture.status_revision;
let callbackId = 0;
let eventId = 0;
let playbackSequence = 0;
const callbacks = new Map<number, MockCallback>();
const listeners = new Map<number, MockEventBinding>();
const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel(MOCK_CHANNEL);

function currentStatus(): RuntimeStatus {
  const status = structuredClone(statusFixture);
  status.status_revision = statusRevision;
  status.media = muted
    ? { state: "unavailable", reason: "AUDIO_MUTED" }
    : structuredClone(statusFixture.media);
  return status;
}

function deliver(event: string, payload: unknown) {
  eventId += 1;
  for (const [id, binding] of listeners) {
    if (binding.event !== event) continue;
    callbacks.get(binding.callbackId)?.({ id, event, payload });
  }
}

function dispatchScenarioEvent(event: unknown) {
  deliver(EVENT_NAME, event);
  channel?.postMessage(event);
}

function emitProbeRequest(payload: unknown) {
  deliver(PROBE_EVENT_NAME, payload);
}

function nextPlaybackId(): string {
  playbackSequence += 1;
  return `pb_mock${String(playbackSequence).padStart(5, "0")}`;
}

function started(playback_id: string, at_ms: number, audio_duration_ms: number, visual_tail_ms: number, playbackClass: PlaybackClass = "ordinary"): PlaybackEvent {
  return {
    kind: "started",
    playback_id,
    class: playbackClass,
    event_type: "multi_kill",
    tier: 3,
    visual_url: null,
    audio_duration_ms,
    visual_tail_ms,
    at_ms,
  };
}

function ended(playback_id: string, at_ms: number, reason: PlaybackEndReason): PlaybackEvent {
  return { kind: "ended", playback_id, reason, at_ms };
}

// --- Mock pack store (synthetic; never changes the readiness shown by runtime_get_status) ---

interface MockPackRecord {
  install_id: string;
  package_id: string;
  revision: number;
  container_sha256: string;
  authority_class: "local_import";
  publisher_claim: "none" | "unverified";
}

const mockRecords: MockPackRecord[] = [];
const mockInstallResults = new Map<string, unknown>();
let mockActiveInstall: string | null = null;
let mockPendingInstalls: string[] = [];
let mockInstallCounter = 0;

/** Tauri 2 rejects a call whose camelCase argument is missing; the mock does the same. */
function requireArgs(command: string, args: Record<string, unknown>, keys: readonly string[]) {
  for (const key of keys) {
    if (!Object.hasOwn(args, key)) {
      throw new Error(`invalid args \`${key}\` for command \`${command}\`: command ${command} missing required key ${key}`);
    }
  }
}

async function mockSha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function mockListing() {
  const activeRecord = mockRecords.find((record) => record.install_id === mockActiveInstall);
  return {
    active: activeRecord
      ? { install_id: activeRecord.install_id, package_id: activeRecord.package_id, revision: activeRecord.revision }
      : null,
    records: structuredClone(mockRecords),
    pending: [...mockPendingInstalls],
  };
}

function mockInstallResult(request_id: string, status: string, reason: string | null, record: MockPackRecord | null) {
  return { schema_version: "1.0.0", request_id, status, reason, record: record ? { ...record } : null };
}

async function mockInspect(args: Record<string, unknown>) {
  requireArgs("media_inspect", args, ["sourcePath"]);
  const path = args.sourcePath;
  if (typeof path !== "string" || path.toLowerCase().includes("missing")) throw "MEDIA_INVALID";
  return { container_sha256: await mockSha256(path), size_bytes: 1_572_864 };
}

async function mockInstall(args: Record<string, unknown>) {
  requireArgs("media_install", args, ["request"]);
  const request = args.request as Record<string, unknown> | undefined;
  const keys = [
    "schema_version", "request_id", "profile_id", "source_path",
    "expected_container_sha256", "project_binding", "activation_policy",
  ];
  if (!request || Object.keys(request).length !== keys.length || !keys.every((key) => Object.hasOwn(request, key))) {
    throw "MEDIA_INVALID";
  }
  if (request.schema_version !== "1.0.0" || request.activation_policy !== "never") throw "MEDIA_INVALID";
  const requestId = String(request.request_id);
  const source = String(request.source_path);
  if (request.expected_container_sha256 !== (await mockSha256(source))) {
    return mockInstallResult(requestId, "rejected", "SOURCE_UNAVAILABLE", null);
  }
  const previous = mockInstallResults.get(requestId);
  if (previous) return structuredClone(previous);

  let result;
  if (source.toLowerCase().includes("bad")) {
    result = mockInstallResult(requestId, "rejected", "MEDIA_INVALID", null);
  } else {
    mockInstallCounter += 1;
    const base = (source.split(/[\\/]/).pop() ?? "pack").replace(/\.[^.]*$/, "").toLowerCase();
    const record: MockPackRecord = {
      install_id: `ins_${String(mockInstallCounter).padStart(10, "0")}`,
      package_id: `mock-${base.replace(/[^a-z0-9_.-]/g, "-").slice(0, 60) || "pack"}`,
      revision: mockInstallCounter,
      container_sha256: String(request.expected_container_sha256),
      authority_class: "local_import",
      publisher_claim: "none",
    };
    mockRecords.push(record);
    result = mockInstallResult(requestId, "installed", null, record);
  }
  mockInstallResults.set(requestId, result);
  return structuredClone(result);
}

function mockActivate(args: Record<string, unknown>) {
  requireArgs("media_activate", args, ["requestId", "installId", "policy"]);
  const requestId = String(args.requestId);
  const installId = String(args.installId);
  const record = mockRecords.find((item) => item.install_id === installId);
  if (!record) return mockInstallResult(requestId, "rejected", "MEDIA_INVALID", null);
  if (mockActiveInstall === installId) return mockInstallResult(requestId, "active", null, record);

  if (args.policy === "now_if_idle") {
    mockActiveInstall = installId;
    mockPendingInstalls = mockPendingInstalls.filter((id) => id !== installId);
    return mockInstallResult(requestId, "active", null, record);
  }
  if (args.policy === "after_match") {
    if (!mockPendingInstalls.includes(installId)) mockPendingInstalls.push(installId);
    // Stand-in for the host resuming a deferred activation once the match barrier ends.
    window.setTimeout(() => {
      if (!mockPendingInstalls.includes(installId)) return;
      mockPendingInstalls = mockPendingInstalls.filter((id) => id !== installId);
      mockActiveInstall = installId;
    }, 7_000);
    return mockInstallResult(requestId, "deferred", "ACTIVATION_DEFERRED", record);
  }
  throw "MEDIA_INVALID";
}

const activeTimers: number[] = [];

function schedule(delay_ms: number, callback: () => void) {
  activeTimers.push(window.setTimeout(callback, delay_ms));
}

// --- Mock audio outcome history (synthetic records in the audio-outcome 1.0.0 shape) ---

type MockOutcomeClass = "critical" | "correction" | "notice" | "ordinary" | "voice";
const mockHistory: Record<string, unknown>[] = [];
let mockHistorySequence = 0;

function recordOutcome(
  event_type: string | null,
  audio_class: MockOutcomeClass,
  outcome: string,
  playback_id: string | null,
  at_ms: number,
) {
  mockHistorySequence += 1;
  mockHistory.push({
    schema_version: "1.0.0",
    instance_id: `ev_g1_s${mockHistorySequence}_0`,
    event_type,
    session_epoch: 1,
    audio_class,
    outcome,
    playback_id,
    // Synthetic fence number; admitted non-voice records must carry an integer.
    package_revision: audio_class === "voice" || playback_id === null ? null : 1,
    decided_monotonic_ms: at_ms,
  });
}

function mockAudioHistory(args: Record<string, unknown>) {
  const limit = typeof args.limit === "number" && args.limit > 0 ? args.limit : mockHistory.length;
  return structuredClone(mockHistory.slice(-limit));
}

function runScenario(name: string) {
  for (const timer of activeTimers.splice(0)) window.clearTimeout(timer);
  const at = Math.floor(performance.now());
  const firstId = nextPlaybackId();

  if (name === "normal") {
    recordOutcome("multi_kill", "ordinary", "admitted", firstId, at);
    dispatchScenarioEvent(started(firstId, at, 900, 0));
    schedule(900, () => dispatchScenarioEvent(ended(firstId, at + 900, "finished")));
  } else if (name === "tail") {
    recordOutcome("multi_kill", "ordinary", "admitted", firstId, at);
    dispatchScenarioEvent(started(firstId, at, 700, 350));
    schedule(1050, () => dispatchScenarioEvent(ended(firstId, at + 1050, "finished")));
  } else if (name === "preempt") {
    recordOutcome("multi_kill", "ordinary", "admitted", firstId, at);
    dispatchScenarioEvent(started(firstId, at, 900, 500));
    schedule(120, () => {
      const secondId = nextPlaybackId();
      recordOutcome("multi_kill", "ordinary", "preempted", firstId, at + 120);
      recordOutcome("multi_kill", "critical", "admitted", secondId, at + 120);
      dispatchScenarioEvent(ended(firstId, at + 120, "cancelled_preempted"));
      dispatchScenarioEvent(started(secondId, at + 120, 650, 0, "critical"));
      schedule(650, () => dispatchScenarioEvent(ended(secondId, at + 770, "finished")));
    });
  } else if (name === "session") {
    recordOutcome("hero_kill", "ordinary", "admitted", firstId, at);
    dispatchScenarioEvent(started(firstId, at, 900, 400));
    schedule(250, () => dispatchScenarioEvent(ended(firstId, at + 250, "cancelled_session")));
  } else if (name === "mute") {
    recordOutcome("hero_kill", "ordinary", "admitted", firstId, at);
    dispatchScenarioEvent(started(firstId, at, 900, 400));
    schedule(200, () => {
      muted = true;
      recordOutcome("hero_death", "ordinary", "muted", null, at + 200);
      statusRevision += 1;
      dispatchScenarioEvent(ended(firstId, at + 200, "cancelled_muted"));
    });
  } else if (name === "stuck") {
    // No verified WebM fixture exists; exercise the missing-ended lifecycle without inventing a visual.
    dispatchScenarioEvent(started(firstId, at, 800, 0));
  } else if (name === "unmapped-danger") {
    // What the product records today: no authorized critical clip exists, so health_danger stays silent.
    recordOutcome("health_danger", "critical", "unmapped", null, at);
  } else if (name === "health-danger") {
    recordOutcome("health_danger", "critical", "admitted", firstId, at);
    dispatchScenarioEvent({
      ...started(firstId, at, 800, 0, "critical"),
      event_type: "health_danger",
      visual_url: null,
    });
  } else if (name === "probe-failure") {
    const path_sha256 = "a".repeat(64);
    emitProbeRequest({
      install_id: "ins_0000000001",
      revision: 1,
      items: [{
        path_sha256,
        url: `kohbor-media://localhost/v1/ins_0000000001/${path_sha256}`,
        duration_ms: 800,
        visual_tail_ms: 0,
      }],
    });
  } else {
    dispatchScenarioEvent({ ...started(firstId, at, 800, 0), playback_id: "bad" });
  }
}

export function installMockHarness() {
  const target = window as unknown as Window & {
    __KOHBOR_MOCK_MEDIA_UNAVAILABLE__?: boolean;
    __TAURI_INTERNALS__?: MockTauriInternals;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: {
      unregisterListener: (event: string, listenerId: number) => void;
    };
  };

  target.__KOHBOR_MOCK_MEDIA_UNAVAILABLE__ = true;
  target.__TAURI_INTERNALS__ = {
    transformCallback(callback) {
      callbackId += 1;
      callbacks.set(callbackId, callback);
      return callbackId;
    },
    async invoke(command, args = {}) {
      if (command === "runtime_get_status") return currentStatus();
      if (command === "own_snapshot") return structuredClone(snapshotFixture);
      if (command === "settings_get") return { ...settingsView };
      if (command === "settings_patch") {
        const request = args.request as { patch?: { locale?: unknown } } | undefined;
        const locale = request?.patch?.locale;
        if (locale !== "th" && locale !== "en") throw new Error("invalid mock locale");
        settingsView = { revision: settingsView.revision + 1, locale };
        return { ...settingsView };
      }
      if (command === "gsi_setup") return null;
      if (command === "set_muted") {
        if (typeof args.muted !== "boolean") throw new Error("set_muted requires a boolean");
        muted = args.muted;
        statusRevision += 1;
        return { muted };
      }
      if (command === "media_probe_report") {
        requireArgs(command, args, ["installId", "revision", "pathSha256", "ok", "durationMs"]);
        window.dispatchEvent(new CustomEvent(MOCK_PROBE_REPORT_EVENT, { detail: args }));
        return { accepted: true };
      }
      if (command === "audio_history") return mockAudioHistory(args);
      if (command === "media_list") return mockListing();
      if (command === "media_inspect") return mockInspect(args);
      if (command === "media_install") return mockInstall(args);
      if (command === "media_activate") return mockActivate(args);
      if (command === "media_probe_pending") return [];
      if (command === "plugin:event|listen") {
        if (typeof args.event !== "string" || typeof args.handler !== "number") {
          throw new Error("invalid mock event listener");
        }
        eventId += 1;
        listeners.set(eventId, { event: args.event, callbackId: args.handler });
        return eventId;
      }
      if (command === "plugin:event|unlisten") {
        if (typeof args.eventId === "number") listeners.delete(args.eventId);
        return null;
      }
      throw new Error(`Unsupported mock command: ${command}`);
    },
  };
  target.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(event, listenerId) {
      const binding = listeners.get(listenerId);
      if (binding?.event !== event) return;
      listeners.delete(listenerId);
      callbacks.delete(binding.callbackId);
    },
  };

  channel?.addEventListener("message", (event: MessageEvent<unknown>) => deliver(EVENT_NAME, event.data));
}

const SCENARIO_NAMES = [
  "normal", "tail", "preempt", "session", "mute", "stuck", "malformed", "health-danger", "unmapped-danger", "probe-failure",
] as const;
type ScenarioName = (typeof SCENARIO_NAMES)[number];

export function MockScenarioSwitcher() {
  const [lastScenario, setLastScenario] = useState<ScenarioName | null>(null);
  const [lastProbeReport, setLastProbeReport] = useState<string | null>(null);
  const th = copy("th").mockScenarios;
  const en = copy("en").mockScenarios;
  const labels: Record<ScenarioName, [string, string]> = {
    normal: [th.normalClip, en.normalClip],
    tail: [th.clipWithTail, en.clipWithTail],
    preempt: [th.preemptCut, en.preemptCut],
    session: [th.cancelSession, en.cancelSession],
    mute: [th.cancelMute, en.cancelMute],
    stuck: [th.stuckWithoutEnd, en.stuckWithoutEnd],
    malformed: [th.malformed, en.malformed],
    "health-danger": ["อันตรายสุขภาพ ไม่มีภาพ", "health_danger, no visual"],
    "unmapped-danger": ["อันตรายสุขภาพ ไม่มีคลิปในแพ็ก (ไม่เล่น)", "health_danger, unmapped (silent)"],
    "probe-failure": ["ทดสอบ probe ล้มเหลว", "Probe unavailable media"],
  };

  useEffect(() => {
    const onReport = (event: Event) => {
      setLastProbeReport(JSON.stringify((event as CustomEvent<unknown>).detail));
    };
    window.addEventListener(MOCK_PROBE_REPORT_EVENT, onReport);
    return () => window.removeEventListener(MOCK_PROBE_REPORT_EVENT, onReport);
  }, []);

  return (
    <aside className="mock-scenario-panel" aria-label={`${th.title} / ${en.title}`}>
      <div className="mock-scenario-heading">
        <h2>{th.title}</h2>
        <span className="muted small">{en.title}</span>
      </div>
      <p>{th.note}</p>
      <p className="muted small">{en.note}</p>
      <div className="mock-scenario-actions">
        {SCENARIO_NAMES.map((name) => (
          <button
            key={name}
            type="button"
            aria-pressed={lastScenario === name}
            onClick={() => {
              setLastScenario(name);
              runScenario(name);
            }}
          >
            <span>{labels[name][0]}</span>
            <span className="muted small">{labels[name][1]}</span>
          </button>
        ))}
      </div>
      <output aria-label="Last media probe report" data-testid="media-probe-report">
        {lastProbeReport ?? "No media probe report yet"}
      </output>
    </aside>
  );
}
