export type ReplayStatus =
  | { state: "available" }
  | { state: "queued" }
  | { state: "parsing" }
  | { state: "ready" }
  | { state: "evicted" }
  | { state: "needs_pack" }
  | { state: "failed"; reason: string };

export type ReplaySource = "dota_replays_folder" | "manual_pick";

export interface ReplayLocalHistory {
  result: "win" | "loss" | null;
  kills: number | null;
  deaths: number | null;
  assists: number | null;
  ended_at_unix_s: number;
}

export interface ReplayLibraryEntry {
  /** Local replay-library key. The renderer uses it internally and never displays it. */
  match_key: string | null;
  match_id: string;
  source: ReplaySource;
  engine_version: string | null;
  parse_time_unix_ms: number | null;
  status: ReplayStatus;
  bundle_bytes: number;
  storage_bytes: number;
  created_at_unix_ms: number;
  match_date_utc: string | null;
  own_hero: string | null;
  duration_s: number | null;
  patch: string | null;
  pinned?: boolean;
  source_exists?: boolean;
  bundle_schema?: "3.0" | "3.1" | null;
  bundle_hash: string | null;
  local_history?: ReplayLocalHistory | null;
}

export interface ReplayJob {
  job_id: string;
  match_id?: string;
  operation: "parse" | "reparse";
  status: ReplayStatus;
}

export interface ReplayLibraryList {
  matches: ReplayLibraryEntry[];
  jobs: ReplayJob[];
  /** Compatibility field; the host does not return a watched-folder path. */
  watched_replay_folder_path: null;
}

export interface ReplayWatcherState {
  folder_found: boolean | null;
  last_scan_unix_ms: number | null;
  last_error_code: string | null;
}

export interface ReplayRetentionPreview {
  evicted_matches: number;
}

export interface ReplayBundle {
  manifest: Record<string, unknown>;
  match_data: Record<string, unknown>;
  world: Record<string, unknown>;
}

export interface ReplayLibraryStateChanged {
  changed: true;
}

export type ReplayStateListener = (
  payload: ReplayLibraryStateChanged,
) => void | Promise<void>;
export type Unlisten = () => void;

export interface ReplayLibraryTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten>;
}

export function createReplayLibraryClient(transport: ReplayLibraryTransport) {
  return {
    async list(): Promise<ReplayLibraryList> {
      const reply = await transport.invoke<Omit<ReplayLibraryList, "watched_replay_folder_path">>("replay_library_list");
      return {
        ...reply,
        watched_replay_folder_path: null,
      };
    },

    pickDemFile(): Promise<string | null> {
      return transport.invoke("replay_library_pick_file");
    },

    watcherState(): Promise<ReplayWatcherState> {
      return transport.invoke("replay_library_watcher_state");
    },

    retentionPreview(keepNewest: number, maxBytes: number): Promise<ReplayRetentionPreview> {
      return transport.invoke("replay_library_retention_preview", { keepNewest, maxBytes });
    },

    retentionSave(keepNewest: number, maxBytes: number, expectedEvictions: number): Promise<unknown> {
      return transport.invoke("replay_library_retention_save", { keepNewest, maxBytes, expectedEvictions });
    },

    open(matchId: string): Promise<ReplayBundle> {
      return transport.invoke("replay_library_open", { matchId });
    },

    parse(idOrPath: string): Promise<ReplayJob> {
      return transport.invoke("replay_library_parse", { idOrPath });
    },

    reparse(matchId: string): Promise<ReplayJob> {
      return transport.invoke("replay_library_reparse", { matchId });
    },

    remove(matchId: string): Promise<void> {
      return transport.invoke("replay_library_remove", { matchId });
    },

    pin(matchId: string, pinned: boolean): Promise<void> {
      return transport.invoke("replay_library_pin", { matchId, pinned });
    },

    clear(): Promise<void> {
      return transport.invoke("replay_library_clear");
    },

    onStateChanged(listener: ReplayStateListener): Promise<Unlisten> {
      return transport.listen<ReplayLibraryStateChanged>(
        "replay-library://state-changed",
        (payload) => {
          void listener(payload);
        },
      );
    },
  };
}
