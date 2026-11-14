// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { LogInvoke } from "./tail.ts";

export interface FrontendErrorPayload { level: "error"; module: "renderer"; msg: string; fields: { source?: string } }
type ErrorKind = "error" | "unhandledrejection";

/** Only message text and a file basename cross IPC. Never serialize arbitrary rejection objects. */
export function shapeFrontendError(kind: ErrorKind, message: unknown, source?: unknown): FrontendErrorPayload {
  const fallback = kind === "error" ? "Renderer error" : "Unhandled promise rejection";
  const msg = (typeof message === "string" ? message.split(/[\r\n]/, 1)[0]?.trim() : null) || fallback;
  const sanitized = msg.replace(/file:\/\/[^\s"'<>]+/gi, "[path]")
    .replace(/\b[A-Za-z]:[\\/][^\s"'<>]+/g, "[path]")
    .replace(/\/(?:Users|home)\/[^\s"'<>]+/g, "[path]").slice(0, 500);
  const filename = typeof source === "string" ? source.split(/[?#]/, 1)[0]?.split(/[\\/]/).at(-1)?.slice(0, 120) : null;
  return { level: "error", module: "renderer", msg: sanitized, fields: filename ? { source: filename } : {} };
}

export function createFrontendForwarder(invoke: LogInvoke, now: () => number = () => performance.now()) {
  let attempts: number[] = [];
  return async (kind: ErrorKind, message: unknown, source?: unknown): Promise<void> => {
    const at = now();
    attempts = attempts.filter(time => at - time < 1000);
    if (attempts.length >= 5) return;
    attempts.push(at);
    try { await invoke("log_frontend", { ...shapeFrontendError(kind, message, source) }); }
    catch { /* Logging must never cause another unhandled rejection, including older hosts. */ }
  };
}

const installed = new WeakSet<EventTarget>();
export function installFrontendLogging(target: EventTarget, invoke: LogInvoke): void {
  if (installed.has(target)) return;
  installed.add(target);
  const forward = createFrontendForwarder(invoke);
  target.addEventListener("error", event => {
    // Resource errors can carry a DOM target; deliberately ignore that target.
    const error = event as ErrorEvent;
    void forward("error", error.message, error.filename);
  });
  target.addEventListener("unhandledrejection", event => {
    const reason: unknown = (event as PromiseRejectionEvent).reason;
    void forward("unhandledrejection", reason instanceof Error ? reason.message : reason);
  });
}
