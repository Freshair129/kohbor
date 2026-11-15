import { listen } from "@tauri-apps/api/event";

type HudEvent = { payload: unknown };
type HudEventListener = (event: string, handler: (event: HudEvent) => void) => Promise<() => void>;

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

const listenHudEvent: HudEventListener = (event, handler) => listen<unknown>(event, handler);

export const subscribeHudState = createHudStateSubscriber(hostAvailable, listenHudEvent);
