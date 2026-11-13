import { listen } from "@tauri-apps/api/event";
import { liveMockRequested } from "./mockMode";

export type LiveEventKind = "capture" | "risk" | "resource";

const EVENT_NAMES: Record<LiveEventKind, string> = {
  capture: "capture://status",
  risk: "risk://state",
  resource: "resource://sample",
};

export async function subscribeLiveEvent(kind: LiveEventKind, onPayload: (payload: unknown) => void): Promise<() => void> {
  if (liveMockRequested()) {
    const mock = await import("./mock");
    if (kind === "capture") return mock.subscribeCapture(onPayload);
    if (kind === "risk") return mock.subscribeRisk(onPayload);
    return mock.subscribeResources(onPayload);
  }
  return listen<unknown>(EVENT_NAMES[kind], (event) => onPayload(event.payload));
}
