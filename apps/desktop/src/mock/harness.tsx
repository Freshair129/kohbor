import { useEffect, useRef, useState } from "react";
import ownSnapshotFixtureRaw from "../../../../fixtures/own-fresh-at-boundary.json?raw";
import runtimeStatusFixtureRaw from "../../../../fixtures/runtime-status-live-valid.json?raw";
import settingsFixtureRaw from "../../../../fixtures/user-settings-valid.json?raw";
import { copy, type Locale } from "../copy";
import type { PlaybackClass, PlaybackEndReason, PlaybackEvent } from "../playback";
import type { OwnSnapshot, RuntimeStatus, UserSettingsView } from "../types";
import { liveMockRequested } from "../live/mockMode";
import { parseOwnSnapshot } from "../ownSnapshot";

type MockCallback = (event: unknown) => void;
type MockEventBinding = { event: string; callbackId: number };
type MockTauriInternals = {
  invoke: (command: string, args?: Record<string, unknown>) => Promise<unknown>;
  transformCallback: (callback: MockCallback) => number;
};

const EVENT_NAME = "playback://event";
const OVERLAY_STATUS_EVENT = "overlay://status";
const OVERLAY_DEMO_EVENT = "overlay://demo";
const PROBE_EVENT_NAME = "media://probe-request";
const MOCK_CHANNEL = "kohbor-mock-playback";
const MOCK_PROBE_REPORT_EVENT = "kohbor-mock-media-probe-report";
const statusFixture = JSON.parse(runtimeStatusFixtureRaw) as RuntimeStatus;
const snapshotFixture = parseOwnSnapshot(JSON.parse(ownSnapshotFixtureRaw));
if (!snapshotFixture) throw new Error("Invalid synthetic own snapshot fixture");
const settingsFixture = JSON.parse(settingsFixtureRaw) as { revision: number; locale: Locale };
let settingsView: UserSettingsView = {
  revision: settingsFixture.revision,
  locale: settingsFixture.locale as Locale,
};
let muted = false;
// Synthetic media readiness for the dev switcher only; never derived from a real pack.
let mediaOverride: RuntimeStatus["media"] | null = null;
let statusRevision = statusFixture.status_revision;
type MockDeckScenario = "standby" | "live" | "novision" | "stale" | "debrief";
type MockSnapshotAvailability = "fresh" | "stale" | "unknown";
let mockDeckScenario: MockDeckScenario = "live";
let mockSnapshotAvailability: MockSnapshotAvailability = "fresh";
let callbackId = 0;
let eventId = 0;
let playbackSequence = 0;
const callbacks = new Map<number, MockCallback>();
const listeners = new Map<number, MockEventBinding>();
const channel = typeof BroadcastChannel === "undefined" ? null : new BroadcastChannel(MOCK_CHANNEL);

function currentStatus(): RuntimeStatus {
  const status = structuredClone(statusFixture);
  status.status_revision = statusRevision;
  if (liveMockRequested()) {
    if (mockDeckScenario === "standby") {
      status.session.phase = "none";
      status.intake.state = "listening";
      status.intake.last_admitted_age_ms = null;
    } else if (mockDeckScenario === "debrief") {
      status.session.phase = "post_game";
      status.intake.state = "receiving";
    } else {
      status.session.phase = "live";
      status.intake.state = mockDeckScenario === "stale" ? "stale" : "receiving";
      status.intake.last_admitted_age_ms = mockDeckScenario === "stale" ? 3_200 : 32;
    }
  }
  // A scenario override wins, like the host's engine-health check which is tested before mute.
  status.media = mediaOverride
    ? { ...mediaOverride }
    : muted
      ? { state: "unavailable", reason: "AUDIO_MUTED" }
      : structuredClone(statusFixture.media);
  return status;
}

function currentOwnSnapshot(): OwnSnapshot {
  const snapshot = structuredClone(snapshotFixture!);
  if (!liveMockRequested()) return snapshot;
  if (mockSnapshotAvailability === "unknown") {
    snapshot.availability = "unknown";
    snapshot.source.age_ms = null;
    snapshot.own = null;
  } else if (mockSnapshotAvailability === "stale") {
    snapshot.availability = "stale";
    snapshot.source.age_ms = 3_200;
    if (snapshot.own) for (const field of Object.values(snapshot.own.fields)) if (field.availability === "fresh") field.availability = "stale";
  } else {
    snapshot.availability = "fresh";
    snapshot.source.age_ms = 32;
  }
  return snapshot;
}

function chooseMockDeckScenario(scenario: MockDeckScenario) {
  mockDeckScenario = scenario;
  mockSnapshotAvailability = scenario === "standby" ? "unknown" : scenario === "stale" ? "stale" : "fresh";
  statusRevision += 1;
  const captureReason = {
    standby: "waiting_for_game",
    live: "live",
    novision: "calibrating",
    stale: "live",
    debrief: "waiting_for_game",
  }[scenario];
  void import("../live/mock").then((driver) => driver.setMockCaptureReason(captureReason));
}

function chooseMockSnapshotAvailability(availability: MockSnapshotAvailability) {
  mockSnapshotAvailability = availability;
  statusRevision += 1;
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
  } else if (name.startsWith("media-")) {
    statusRevision += 1;
    if (name === "media-unmapped") mediaOverride = { state: "not_ready", reason: "MEDIA_UNMAPPED" };
    else if (name === "media-invalid") mediaOverride = { state: "not_ready", reason: "MEDIA_INVALID" };
    else if (name === "media-probe") mediaOverride = { state: "not_ready", reason: "MEDIA_PROBE_FAILED" };
    else if (name === "media-device") mediaOverride = { state: "unavailable", reason: "CAPABILITY_UNAVAILABLE" };
    else if (name === "media-ready") mediaOverride = { state: "ready", reason: null };
    else {
      mediaOverride = null;
      muted = false;
    }
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
      // Explicit synthetic build metadata; never a production-version fallback.
      if (command === "app_version") return { version: "0.0.0-dev-sample", git_commit: null, build_date: null, profile: "debug", demo: true };
      if (command === "own_snapshot") return currentOwnSnapshot();
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
      if (command === "plugin:event|emit") {
        if (typeof args.event !== "string" || !Object.hasOwn(args, "payload")) {
          throw new Error("invalid mock event emission");
        }
        deliver(args.event, args.payload);
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

export function OverlayMockHarness() {
  const [lastPlaybackId, setLastPlaybackId] = useState<string | null>(null);
  const timers = useRef<number[]>([]);

  const runOverlayDemo = () => {
    for (const timer of timers.current) window.clearTimeout(timer);
    timers.current = [];
    const at = Math.floor(performance.now());
    const playback_id = nextPlaybackId();
    deliver(OVERLAY_STATUS_EVENT, { media: { state: "not_ready", reason: "MEDIA_UNMAPPED" } });
    deliver(OVERLAY_DEMO_EVENT, { source: "demo" });
    dispatchScenarioEvent({
      ...started(playback_id, at, 3_000, 0),
      visual_url: `kohbor-media://localhost/v1/ins_0000000001/${"a".repeat(64)}`,
    });
    setLastPlaybackId(playback_id);
    timers.current.push(window.setTimeout(() => {
      dispatchScenarioEvent(ended(playback_id, at + 3_000, "finished"));
    }, 3_000));
  };

  useEffect(() => {
    const timer = window.setTimeout(runOverlayDemo, 100);
    timers.current.push(timer);
    return () => {
      for (const active of timers.current) window.clearTimeout(active);
      timers.current = [];
    };
  }, []);

  return (
    <aside className="overlay-mock-panel" aria-label="ตัวอย่าง overlay แบบสังเคราะห์">
      <p>DEMO preview only · no game detection or pack readiness is inferred.</p>
      <button type="button" onClick={runOverlayDemo}>เล่นตัวอย่าง DEMO อีกครั้ง</button>
      <output>{lastPlaybackId ?? "กำลังเตรียมตัวอย่าง"}</output>
    </aside>
  );
}

const SCENARIO_NAMES = [
  "normal", "tail", "preempt", "session", "mute", "stuck", "malformed", "health-danger", "unmapped-danger", "probe-failure",
  "media-unmapped", "media-invalid", "media-probe", "media-device", "media-ready", "media-reset",
] as const;
type ScenarioName = (typeof SCENARIO_NAMES)[number];

export function MockScenarioSwitcher() {
  const [lastScenario, setLastScenario] = useState<ScenarioName | null>(null);
  const [lastProbeReport, setLastProbeReport] = useState<string | null>(null);
  const [deckScenario, setDeckScenario] = useState<MockDeckScenario>("live");
  const [snapshotAvailability, setSnapshotAvailability] = useState<MockSnapshotAvailability>("fresh");
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
    "media-unmapped": ["สถานะจำลอง: ไม่มีคลิปสำคัญ", "Mock status: MEDIA_UNMAPPED"],
    "media-invalid": ["สถานะจำลอง: สื่อไม่ผ่านตรวจ", "Mock status: MEDIA_INVALID"],
    "media-probe": ["สถานะจำลอง: ตรวจภาพล้มเหลว", "Mock status: MEDIA_PROBE_FAILED"],
    "media-device": ["สถานะจำลอง: ระบบเสียงใช้ไม่ได้", "Mock status: CAPABILITY_UNAVAILABLE"],
    "media-ready": ["สถานะจำลอง: พร้อม (สังเคราะห์ ไม่ใช่ของจริง)", "Mock status: ready (synthetic)"],
    "media-reset": ["คืนสถานะจำลองเป็นค่าเริ่มต้น", "Reset mock media status"],
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
      {liveMockRequested() && (
        <section className="mock-deck-controls" aria-label="ตัวอย่างสถานะ Command Deck / Command Deck sample states">
          <strong>ตัวอย่างข้อมูลสำหรับพัฒนา — ไม่ใช่ข้อมูลจากเกมจริง</strong>
          <span className="muted small">DEV sample state — no real game data</span>
          <label>
            สถานะ Command Deck / Deck state
            <select
              value={deckScenario}
              onChange={(event) => {
                const next = event.currentTarget.value as MockDeckScenario;
                setDeckScenario(next);
                const defaultAvailability = next === "standby" ? "unknown" : next === "stale" ? "stale" : "fresh";
                setSnapshotAvailability(defaultAvailability);
                chooseMockDeckScenario(next);
              }}
            >
              <option value="standby">รอข้อมูล / Standby</option>
              <option value="live">สด / Live</option>
              <option value="novision">ไม่มีภาพ / No vision</option>
              <option value="stale">ข้อมูลเก่า / Stale</option>
              <option value="debrief">หลังเกม / Debrief</option>
            </select>
          </label>
          <label>
            ความสดของ snapshot ตัวคุณ / Own snapshot
            <select
              value={snapshotAvailability}
              onChange={(event) => {
                const next = event.currentTarget.value as MockSnapshotAvailability;
                setSnapshotAvailability(next);
                chooseMockSnapshotAvailability(next);
              }}
            >
              <option value="fresh">สด / Fresh</option>
              <option value="stale">เก่า / Stale</option>
              <option value="unknown">ไม่ทราบ / Unknown</option>
            </select>
          </label>
        </section>
      )}
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
