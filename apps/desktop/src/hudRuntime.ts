import { listen } from "@tauri-apps/api/event";

type HudEvent = { payload: unknown };
type HudEventListener = (event: string, handler: (event: HudEvent) => void) => Promise<() => void>;
const HUD_LAYOUT_MOCK_CHANNEL = "kohbor-mock-hud-layout";

function hostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function createHudStateSubscriber(
  available: () => boolean,
  listenEvent: HudEventListener,
): (onPayload: (payload: unknown) => void) => Promise<() => void> {
  return (onPayload) => {
    if (!available()) return Promise.resolve(() => {});
    return listenEvent("hud://state", (event) => onPayload(event.payload));
  };
}

export function createHudLayoutSubscriber(
  available: () => boolean,
  listenEvent: HudEventListener,
): (onPayload: (payload: unknown) => void) => Promise<() => void> {
  return (onPayload) => {
    if (!available()) return Promise.resolve(() => {});
    return listenEvent("hud://layout", (event) => onPayload(event.payload));
  };
}

const listenHudEvent: HudEventListener = (event, handler) => listen<unknown>(event, handler);

export const subscribeHudState = createHudStateSubscriber(hostAvailable, listenHudEvent);
const subscribeHostHudLayout = createHudLayoutSubscriber(hostAvailable, listenHudEvent);

export function subscribeHudLayout(onPayload: (payload: unknown) => void): Promise<() => void> {
  const mockMode = typeof window !== "undefined"
    && import.meta.env.DEV
    && new URLSearchParams(window.location.search).get("mock") === "1"
    && typeof BroadcastChannel !== "undefined";
  if (!mockMode) return subscribeHostHudLayout(onPayload);

  const channel = new BroadcastChannel(HUD_LAYOUT_MOCK_CHANNEL);
  const onMessage = (event: MessageEvent<unknown>) => onPayload(event.data);
  channel.addEventListener("message", onMessage);
  const close = () => {
    channel.removeEventListener("message", onMessage);
    channel.close();
  };
  return subscribeHostHudLayout(onPayload).then((unlistenHost) => () => {
    unlistenHost();
    close();
  }, (reason: unknown) => {
    close();
    throw reason;
  });
}
