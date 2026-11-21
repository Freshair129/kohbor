import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { parseDraftColumnView, type DraftColumnView } from "./draftColumnModel";

export function draftHostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const draftColumnApi = {
  getView: async (): Promise<DraftColumnView> => parseDraftColumnView(await invoke<unknown>("draft_column_view")),
  getPrefs: (): Promise<unknown> => invoke<unknown>("draft_stats_prefs_get"),
  setPrefs: (patch: Record<string, unknown>): Promise<unknown> => invoke<unknown>("draft_stats_prefs_set", { patch }),
  retry: (): Promise<unknown> => invoke<unknown>("draft_stats_retry"),
};

export async function subscribeDraftView(callback: (view: DraftColumnView) => void): Promise<() => void> {
  if (!draftHostAvailable()) return () => {};
  return listen<unknown>("draft://view", (event) => callback(parseDraftColumnView(event.payload)));
}

export function commandErrorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
