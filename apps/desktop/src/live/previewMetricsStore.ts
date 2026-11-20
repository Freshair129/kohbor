import { useSyncExternalStore } from "react";
import { unknownPreviewMetrics, type PreviewMetrics } from "./preview";

let metrics = unknownPreviewMetrics();
const subscribers = new Set<() => void>();

export function publishPreviewMetrics(next: PreviewMetrics) {
  metrics = next;
  for (const subscriber of subscribers) subscriber();
}

export function clearPreviewMetrics() {
  publishPreviewMetrics(unknownPreviewMetrics());
}

export function getPreviewMetrics(): PreviewMetrics {
  return metrics;
}

export function subscribePreviewMetrics(onChange: () => void) {
  subscribers.add(onChange);
  return () => subscribers.delete(onChange);
}

export function usePreviewMetrics(): PreviewMetrics {
  return useSyncExternalStore(subscribePreviewMetrics, getPreviewMetrics, getPreviewMetrics);
}
