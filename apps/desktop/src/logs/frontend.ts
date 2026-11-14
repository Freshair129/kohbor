// @ts-ignore Explicit extensions support Node's TypeScript loader.
import type { LogInvoke } from "./tail.ts";

export interface FrontendErrorPayload { level: "error"; module: "renderer"; msg: string; fields: { source?: string; code?: string } }
type ErrorKind = "error" | "unhandledrejection";

/** Arbitrary error text can contain a player or Steam name, so it is NEVER forwarded: only a fixed
 * message, a file basename and a short hash of the original text cross IPC (the hash lets the same
 * error be recognised without revealing it). Never serialize arbitrary rejection objects. */
function shortHash(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) { h ^= text.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h.toString(16).padStart(8, "0");
}

export function shapeFrontendError(kind: ErrorKind, message: unknown, source?: unknown): FrontendErrorPayload {
  const fallback = kind === "error" ? "Renderer error" : "Unhandled promise rejection";
  const original = typeof message === "string" ? message.slice(0, 2000) : "";
  const filename = typeof source === "string" ? source.split(/[?#]/, 1)[0]?.split(/[\/]/).at(-1)?.slice(0, 120) : null;
  const fields: { source?: string; code?: string } = {};
  if (filename) fields.source = filename;
  if (original) fields.code = shortHash(original);
  return { level: "error", module: "renderer", msg: fallback, fields };
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
