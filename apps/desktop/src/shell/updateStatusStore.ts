import { parseReleasePageReply, parseUpdateReply, type ReleasePageReply, type UpdateReply } from "./updateModel";

export type UpdateCommand =
  | "update_status"
  | "update_check"
  | "update_download"
  | "update_install"
  | "update_open_release_page";
export type UpdateInvoke = (command: UpdateCommand, args?: Record<string, unknown>) => Promise<unknown>;

export interface UpdateStatusSnapshot {
  reply: UpdateReply | null;
  error: boolean;
  pending: UpdateCommand | null;
  /** Result of the last "open the release page" fallback action. */
  page?: ReleasePageReply | null;
  /** The session-only consent toggle defaults off because current settings persist locale only. */
  automaticChecksEnabled?: boolean;
  setAutomaticChecksEnabled?: (enabled: boolean) => Promise<void>;
  /** Separate user action from installation; the host keeps download and install as separate stages. */
  download?: () => Promise<void>;
}

const AUTO_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1_000;
const AUTO_CHECK_LAST_AT_KEY = "kohbor.update.last-automatic-check-at";
let autoCheckAttemptedThisStart = false;
let lastAutoCheckAt: number | null = null;

function readLastAutoCheckAt(): number | null {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem(AUTO_CHECK_LAST_AT_KEY);
      if (stored !== null) {
        const timestamp = Number(stored);
        if (Number.isSafeInteger(timestamp) && timestamp > 0) return timestamp;
      }
    } catch {
      // Local storage can be unavailable in restricted browser contexts; retain the process limit.
    }
  }
  return lastAutoCheckAt;
}

function rememberLastAutoCheckAt(timestamp: number): void {
  lastAutoCheckAt = timestamp;
  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(AUTO_CHECK_LAST_AT_KEY, String(timestamp));
    } catch {
      // The per-app-start guard remains active when persistent storage is unavailable.
    }
  }
}

export function createUpdateStatusStore(invoke: UpdateInvoke) {
  let snapshot: UpdateStatusSnapshot = {
    reply: null,
    error: false,
    pending: null,
    automaticChecksEnabled: false,
  };
  let mounted = false;
  let statusPoll: number | null = null;
  const listeners = new Set<() => void>();

  const publish = (next: UpdateStatusSnapshot) => {
    snapshot = next;
    for (const listener of listeners) listener();
  };

  const request = async (command: UpdateCommand, args?: Record<string, unknown>): Promise<void> => {
    if (snapshot.pending !== null) return;
    publish({ ...snapshot, pending: command, error: false });
    const statusPoll = command === "update_status" ? null : setInterval(() => {
      void invoke("update_status").then(parseUpdateReply).then((reply) => {
        if (reply !== null) publish({ ...snapshot, reply });
      }).catch(() => {});
    }, 300);
    try {
      const reply = parseUpdateReply(await invoke(command, args));
      publish({ reply, error: false, pending: null });
    } catch {
      // Keep the last parsed host state and expose only a short localized error line.
      publish({ ...snapshot, error: true, pending: null });
    } finally {
      if (statusPoll !== null) clearInterval(statusPoll);
    }
  };

  const setAutomaticChecksEnabled = async (enabled: boolean): Promise<void> => {
    publish({ ...snapshot, automaticChecksEnabled: enabled });
    if (!enabled || snapshot.pending !== null || autoCheckAttemptedThisStart) return;
    if (snapshot.reply?.match_active || snapshot.reply?.reason === "MATCH_ACTIVE") return;
    const now = Date.now();
    const lastCheck = readLastAutoCheckAt();
    if (lastCheck !== null && now - lastCheck < AUTO_CHECK_INTERVAL_MS) return;

    autoCheckAttemptedThisStart = true;
    rememberLastAutoCheckAt(now);
    // Consent only authorizes a check. The automatic path never invokes download or install.
    await request("update_check");
  };

  const refreshStatus = async (): Promise<void> => {
    if (snapshot.pending !== null) return;
    try {
      const reply = parseUpdateReply(await invoke("update_status"));
      if (reply !== null) publish({ ...snapshot, reply, error: false });
    } catch {
      // Status polling is only for match-state freshness; keep the last readable state on failure.
    }
  };

  const download = async (): Promise<void> => {
    if (snapshot.pending !== null || snapshot.reply?.state !== "available"
      || snapshot.reply.match_active || snapshot.reply.reason === "MATCH_ACTIVE") return;
    await request("update_download");
  };

  snapshot = { ...snapshot, setAutomaticChecksEnabled, download };

  return {
    getSnapshot: () => snapshot,
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    mount: async () => {
      if (mounted) return;
      mounted = true;
      await request("update_status");
      if (mounted && typeof window !== "undefined" && statusPoll === null) {
        statusPoll = window.setInterval(() => {
          const state = snapshot.reply?.state;
          if (snapshot.reply?.match_active || state === "checking" || state === "downloading" || state === "installing") {
            void refreshStatus();
          }
        }, 1_000);
      }
    },
    unmount: () => {
      mounted = false;
      if (statusPoll !== null && typeof window !== "undefined") {
        window.clearInterval(statusPoll);
        statusPoll = null;
      }
    },
    /** Opens the GitHub latest-release page in the default browser; never downloads or installs. */
    openReleasePage: async (): Promise<void> => {
      if (snapshot.pending !== null || snapshot.reply?.match_active || snapshot.reply?.reason === "MATCH_ACTIVE") return;
      publish({ ...snapshot, pending: "update_open_release_page", error: false });
      try {
        const page = parseReleasePageReply(await invoke("update_open_release_page"));
        publish({ ...snapshot, page, error: page === null, pending: null });
      } catch {
        publish({ ...snapshot, error: true, pending: null });
      }
    },
    check: async () => {
      if (snapshot.pending !== null || snapshot.reply?.match_active || snapshot.reply?.reason === "MATCH_ACTIVE") return;
      await request("update_check");
    },
    install: async () => {
      if (snapshot.pending !== null || snapshot.reply?.state !== "ready" || snapshot.reply.match_active
        || snapshot.reply.reason === "MATCH_ACTIVE") return;
      await request("update_install", { confirm: true });
    },
    download,
    setAutomaticChecksEnabled,
  };
}

export type UpdateStatusStore = ReturnType<typeof createUpdateStatusStore>;
