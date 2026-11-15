import { parseReleasePageReply, parseUpdateReply, type ReleasePageReply, type UpdateReply } from "./updateModel";

export type UpdateCommand = "update_status" | "update_check" | "update_install" | "update_open_release_page";
export type UpdateInvoke = (command: UpdateCommand, args?: Record<string, unknown>) => Promise<unknown>;

export interface UpdateStatusSnapshot {
  reply: UpdateReply | null;
  error: boolean;
  pending: UpdateCommand | null;
  /** Result of the last "open the release page" action (the only live update action in this build). */
  page?: ReleasePageReply | null;
}

export function createUpdateStatusStore(invoke: UpdateInvoke) {
  let snapshot: UpdateStatusSnapshot = { reply: null, error: false, pending: null };
  let mounted = false;
  const listeners = new Set<() => void>();

  const publish = (next: UpdateStatusSnapshot) => {
    snapshot = next;
    for (const listener of listeners) listener();
  };

  const request = async (command: UpdateCommand, args?: Record<string, unknown>): Promise<void> => {
    if (snapshot.pending !== null) return;
    publish({ ...snapshot, pending: command, error: false });
    try {
      const reply = parseUpdateReply(await invoke(command, args));
      publish({ reply, error: false, pending: null });
    } catch {
      // Keep the last parsed host state and expose only a short localized error line.
      publish({ ...snapshot, error: true, pending: null });
    }
  };

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
      if (snapshot.pending !== null || snapshot.reply?.state !== "available" || snapshot.reply.match_active
        || snapshot.reply.reason === "MATCH_ACTIVE") return;
      await request("update_install", { confirm: true });
    },
  };
}

export type UpdateStatusStore = ReturnType<typeof createUpdateStatusStore>;
