import { useSyncExternalStore } from "react";
import { subscribeLiveEvent } from "./events";
import { useFreshValue, type TimedValue } from "./freshness";
import { addResourceSample, emptyResourceSummary, freshResourceMetrics, parseResourceMetrics } from "./state";
import type { ResourceMetrics, ResourceSummary } from "./types";

export interface ResourcePanelState {
  current: ResourceMetrics | null;
  summary: ResourceSummary;
  reset: () => void;
}

interface ResourceStore {
  timed: TimedValue<ResourceMetrics>;
  summary: ResourceSummary;
}

// Keep the session summary and one listener for this webview even when the Live route unmounts.
let store: ResourceStore = {
  timed: { value: null, receivedAt: null },
  summary: emptyResourceSummary(),
};
let listening = false;
const storeSubscribers = new Set<() => void>();

function notifyStore() {
  for (const subscriber of storeSubscribers) subscriber();
}

function startResourceListener() {
  if (listening) return;
  listening = true;
  void subscribeLiveEvent("resource", (payload) => {
    const parsed = parseResourceMetrics(payload);
    const receivedAt = performance.now();
    store = {
      timed: { value: parsed, receivedAt },
      summary: parsed ? addResourceSample(store.summary, parsed) : store.summary,
    };
    notifyStore();
  }).catch(() => {
    // Resource values stay unknown when their listener cannot be established.
    listening = false;
  });
}

function subscribeStore(onChange: () => void): () => void {
  storeSubscribers.add(onChange);
  startResourceListener();
  return () => storeSubscribers.delete(onChange);
}

function getStoreSnapshot(): ResourceStore {
  return store;
}

function resetSessionSummary() {
  store = { ...store, summary: emptyResourceSummary() };
  notifyStore();
}

export function useResources(): ResourcePanelState {
  const snapshot = useSyncExternalStore(subscribeStore, getStoreSnapshot, getStoreSnapshot);
  const sample = useFreshValue(snapshot.timed);
  const current = freshResourceMetrics(sample, snapshot.timed.receivedAt, performance.now());
  return { current, summary: snapshot.summary, reset: resetSessionSummary };
}
