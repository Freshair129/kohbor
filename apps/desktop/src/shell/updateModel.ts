import { updateCopy } from "./updateCopy";
import type { Locale } from "../copy";

export const UPDATE_STATES = [
  "not_configured",
  "idle",
  "checking",
  "up_to_date",
  "available",
  "downloading",
  "ready_to_install",
  "failed",
] as const;

export type UpdateState = (typeof UPDATE_STATES)[number];

export interface UpdateReply {
  state: UpdateState;
  current_version: string;
  reason: string | null;
  available: { version: string; notes: string; pub_date: string | null } | null;
  match_active: boolean;
}

export interface UpdateView {
  headline: string;
  detail: string;
  canCheck: boolean;
  canInstall: boolean;
  tone: "unknown" | "neutral" | "positive" | "warning" | "critical";
}

function object(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}

/** Parse the host reply without defaults; malformed payloads stay unknown. */
export function parseUpdateReply(input: unknown): UpdateReply | null {
  const value = object(input);
  if (!value || !exactKeys(value, ["state", "current_version", "reason", "available", "match_active"])) return null;
  if (typeof value.state !== "string" || !(UPDATE_STATES as readonly string[]).includes(value.state)) return null;
  if (typeof value.current_version !== "string") return null;
  if (value.reason !== null && typeof value.reason !== "string") return null;
  if (typeof value.match_active !== "boolean") return null;

  let available: UpdateReply["available"] = null;
  if (value.available !== null) {
    const candidate = object(value.available);
    if (!candidate || !exactKeys(candidate, ["version", "notes", "pub_date"])) return null;
    if (typeof candidate.version !== "string" || typeof candidate.notes !== "string") return null;
    if (candidate.pub_date !== null && typeof candidate.pub_date !== "string") return null;
    available = { version: candidate.version, notes: candidate.notes, pub_date: candidate.pub_date };
  }
  if (value.state === "available" && available === null) return null;

  return {
    state: value.state as UpdateState,
    current_version: value.current_version,
    reason: value.reason,
    available,
    match_active: value.match_active,
  };
}

function reasonText(code: string | null, locale: Locale): string {
  const text = updateCopy(locale);
  if (code === "MATCH_ACTIVE") return text.matchActive;
  if (code === "BUILD_WITHOUT_UPDATER") return text.buildWithoutUpdater;
  if (code === "NO_ENDPOINT") return text.noEndpoint;
  if (code === "NO_PUBKEY") return text.noPubkey;
  if (code === "NETWORK") return text.network;
  if (code === "SIGNATURE") return text.signature;
  if (code === "BACKEND_UNAVAILABLE") return text.backendUnavailable;
  if (code === "BAD_RESPONSE") return text.badResponse;
  return text.failedUnknownReason;
}

export function updateView(reply: UpdateReply | null, locale: Locale = "th"): UpdateView {
  const text = updateCopy(locale);
  let view: UpdateView;
  if (!reply) {
    view = { headline: text.unknown, detail: text.unknownDetail, canCheck: true, canInstall: false, tone: "unknown" };
  } else {
    switch (reply.state) {
      case "not_configured":
        view = { headline: text.notConfigured, detail: reasonText(reply.reason, locale), canCheck: false, canInstall: false, tone: "warning" };
        break;
      case "idle":
        view = { headline: text.idle, detail: "", canCheck: true, canInstall: false, tone: "neutral" };
        break;
      case "checking":
        view = { headline: text.checking, detail: text.checkingDetail, canCheck: false, canInstall: false, tone: "neutral" };
        break;
      case "up_to_date":
        view = { headline: text.upToDate, detail: text.upToDateDetail, canCheck: true, canInstall: false, tone: "positive" };
        break;
      case "available":
        view = {
          headline: text.available(reply.available?.version ?? ""),
          detail: reply.available?.notes ?? "",
          canCheck: true,
          canInstall: reply.available !== null,
          tone: "warning",
        };
        break;
      case "downloading":
        view = { headline: text.downloading, detail: text.downloadingDetail, canCheck: false, canInstall: false, tone: "neutral" };
        break;
      case "ready_to_install":
        view = { headline: text.readyToInstall, detail: "", canCheck: false, canInstall: false, tone: "positive" };
        break;
      case "failed":
        view = { headline: text.failed, detail: reasonText(reply.reason, locale), canCheck: true, canInstall: false, tone: "critical" };
        break;
    }
  }

  if (reply?.match_active || reply?.reason === "MATCH_ACTIVE") {
    return { ...view, detail: text.matchActive, canCheck: false, canInstall: false };
  }
  return view;
}

/** Two explicit user actions are required before the install callback can run. */
export function createUpdateConfirmFlow() {
  let confirming = false;
  return {
    isOpen: () => confirming,
    begin: () => { confirming = true; },
    cancel: () => { confirming = false; },
    confirm: async (install: () => Promise<unknown>): Promise<boolean> => {
      if (!confirming) return false;
      confirming = false;
      await install();
      return true;
    },
  };
}

/** Reply of the open-release-page action (option 1: the button only opens the GitHub latest release page). */
export interface ReleasePageReply {
  opened: boolean;
  reason: string | null;
  url: string | null;
}

export function parseReleasePageReply(input: unknown): ReleasePageReply | null {
  const value = object(input);
  if (!value || !exactKeys(value, ["opened", "reason", "url"])) return null;
  if (typeof value.opened !== "boolean") return null;
  if (value.reason !== null && typeof value.reason !== "string") return null;
  if (value.url !== null && typeof value.url !== "string") return null;
  return { opened: value.opened, reason: value.reason, url: value.url };
}

/** Result line for the release-page button; null = nothing to say yet. */
export function releasePageMessage(page: ReleasePageReply | null, locale: Locale = "th"): string | null {
  const text = updateCopy(locale);
  if (page === null) return null;
  if (page.opened) return text.pageOpened;
  return page.reason === "MATCH_ACTIVE" ? text.matchActive : text.pageFailed;
}
