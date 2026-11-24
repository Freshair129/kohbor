export type ReplayStatus =
  | { state: "available" }
  | { state: "queued" }
  | { state: "parsing" }
  | { state: "ready" }
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
  match_id: string;
  source: ReplaySource;
  source_path: string;
  dem_sha256: string | null;
  engine_version: string | null;
  engine_sha256: string | null;
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
  local_history?: ReplayLocalHistory | null;
}

export interface ReplayJob {
  job_id: string;
  status: ReplayStatus;
}

export interface ReplayLibraryList {
  matches: ReplayLibraryEntry[];
  jobs: ReplayJob[];
  watched_replay_folder_path: string | null;
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
    list(): Promise<ReplayLibraryList> {
      return transport.invoke("replay_library_list");
    },

    pickDemFile(): Promise<string | null> {
      return transport.invoke("replay_library_pick_file");
    },

    open(matchId: string): Promise<ReplayBundle> {
      return transport.invoke("replay_library_open", { matchId });
    },

    parse(idOrPath: string): Promise<ReplayJob> {
      return transport.invoke("replay_library_parse", { idOrPath });
    },

    remove(matchId: string): Promise<void> {
      return transport.invoke("replay_library_remove", { matchId });
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
