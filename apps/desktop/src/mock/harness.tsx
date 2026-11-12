import { useState } from "react";
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
const MOCK_CHANNEL = "kohbor-mock-playback";
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

function deliver(payload: unknown) {
  eventId += 1;
  for (const [id, binding] of listeners) {
    if (binding.event !== EVENT_NAME) continue;
    callbacks.get(binding.callbackId)?.({ id, event: binding.event, payload });
  }
}

function dispatchScenarioEvent(event: unknown) {
  deliver(event);
  channel?.postMessage(event);
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

const activeTimers: number[] = [];

function schedule(delay_ms: number, callback: () => void) {
  activeTimers.push(window.setTimeout(callback, delay_ms));
}

function runScenario(name: string) {
  for (const timer of activeTimers.splice(0)) window.clearTimeout(timer);
  const at = Math.floor(performance.now());
  const firstId = nextPlaybackId();

  if (name === "normal") {
    dispatchScenarioEvent(started(firstId, at, 900, 0));
    schedule(900, () => dispatchScenarioEvent(ended(firstId, at + 900, "finished")));
  } else if (name === "tail") {
    dispatchScenarioEvent(started(firstId, at, 700, 350));
    schedule(1050, () => dispatchScenarioEvent(ended(firstId, at + 1050, "finished")));
  } else if (name === "preempt") {
    dispatchScenarioEvent(started(firstId, at, 900, 500));
    schedule(120, () => {
      const secondId = nextPlaybackId();
      dispatchScenarioEvent(ended(firstId, at + 120, "cancelled_preempted"));
      dispatchScenarioEvent(started(secondId, at + 120, 650, 0, "critical"));
      schedule(650, () => dispatchScenarioEvent(ended(secondId, at + 770, "finished")));
    });
  } else if (name === "session") {
    dispatchScenarioEvent(started(firstId, at, 900, 400));
    schedule(250, () => dispatchScenarioEvent(ended(firstId, at + 250, "cancelled_session")));
  } else if (name === "mute") {
    dispatchScenarioEvent(started(firstId, at, 900, 400));
    schedule(200, () => {
      muted = true;
      statusRevision += 1;
      dispatchScenarioEvent(ended(firstId, at + 200, "cancelled_muted"));
    });
  } else if (name === "stuck") {
    // No verified WebM fixture exists; exercise the missing-ended lifecycle without inventing a visual.
    dispatchScenarioEvent(started(firstId, at, 800, 0));
  } else {
    dispatchScenarioEvent({ ...started(firstId, at, 800, 0), playback_id: "bad" });
  }
}

export function installMockHarness() {
  const target = window as unknown as Window & {
    __TAURI_INTERNALS__?: MockTauriInternals;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: {
      unregisterListener: (event: string, listenerId: number) => void;
    };
  };

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

  channel?.addEventListener("message", (event: MessageEvent<unknown>) => deliver(event.data));
}

const SCENARIO_NAMES = ["normal", "tail", "preempt", "session", "mute", "stuck", "malformed"] as const;
type ScenarioName = (typeof SCENARIO_NAMES)[number];

export function MockScenarioSwitcher() {
  const [lastScenario, setLastScenario] = useState<ScenarioName | null>(null);
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
  };

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
    </aside>
  );
}
