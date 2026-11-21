import type { DraftColumnView } from "./draftColumnModel";

export const DRAFT_VIEW_POLL_INTERVAL_MS = 2000;

export interface DraftViewPollEnvironment {
  isVisible(): boolean;
  subscribeVisibility(listener: () => void): () => void;
  setInterval(callback: () => void, intervalMs: number): unknown;
  clearInterval(handle: unknown): void;
}

export interface DraftViewRequestSequence {
  next(): number;
  isCurrent(request: number): boolean;
  invalidate(): void;
}

export function createDraftViewRequestSequence(): DraftViewRequestSequence {
  let latest = 0;
  return {
    next: () => ++latest,
    isCurrent: (request) => request === latest,
    invalidate: () => { latest += 1; },
  };
}

function browserEnvironment(): DraftViewPollEnvironment {
  return {
    isVisible: () => document.visibilityState === "visible",
    subscribeVisibility: (listener) => {
      document.addEventListener("visibilitychange", listener);
      return () => document.removeEventListener("visibilitychange", listener);
    },
    setInterval: (callback, intervalMs) => window.setInterval(callback, intervalMs),
    clearInterval: (handle) => window.clearInterval(handle as number),
  };
}

/** Polls only while the Settings section's document is visible; injected scheduling keeps this testable. */
export function startDraftViewPolling(
  getView: () => Promise<DraftColumnView>,
  onView: (view: DraftColumnView) => void,
  onError: (error: unknown) => void,
  environment: DraftViewPollEnvironment = browserEnvironment(),
  requests: DraftViewRequestSequence = createDraftViewRequestSequence(),
): () => void {
  let disposed = false;
  let timerActive = false;
  let timer: unknown;

  const poll = async () => {
    if (disposed || !environment.isVisible()) return;
    const request = requests.next();
    try {
      const view = await getView();
      if (!disposed && environment.isVisible() && requests.isCurrent(request)) onView(view);
    } catch (error) {
      if (!disposed && environment.isVisible() && requests.isCurrent(request)) onError(error);
    }
  };

  const visibilityChanged = () => {
    if (disposed) return;
    if (environment.isVisible()) {
      if (!timerActive) {
        timer = environment.setInterval(() => { void poll(); }, DRAFT_VIEW_POLL_INTERVAL_MS);
        timerActive = true;
        void poll();
      }
    } else if (timerActive) {
      requests.invalidate();
      environment.clearInterval(timer);
      timer = undefined;
      timerActive = false;
    }
  };

  const unsubscribe = environment.subscribeVisibility(visibilityChanged);
  visibilityChanged();
  return () => {
    disposed = true;
    unsubscribe();
    if (timerActive) environment.clearInterval(timer);
    timer = undefined;
    timerActive = false;
  };
}
