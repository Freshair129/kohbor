// IPC for the Overview page. The renderer never supplies a URL, host or path: news links are
// opened by post id and link index, and the host resolves them from the validated feed.
import { invoke, isTauri } from "@tauri-apps/api/core";

function host<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) return Promise.reject("CAPABILITY_UNAVAILABLE");
  return invoke<T>(command, args);
}

export const overviewClient = {
  matchHistory: () => host<unknown>("match_history_get"),
  clearMatchHistory: () => host<unknown>("match_history_clear"),
  news: () => host<unknown>("news_feed_get"),
  refreshNews: () => host<unknown>("news_refresh"),
  newsPrefs: () => host<{ enabled: boolean; disclosure_seen: boolean }>("news_prefs_get"),
  setNewsPrefs: (patch: { enabled?: boolean; disclosure_seen?: boolean }) =>
    host<{ enabled: boolean; disclosure_seen: boolean }>("news_prefs_set", { patch }),
  openNewsLink: (postId: string, index: number) =>
    host<{ opened: boolean; reason: string | null }>("news_open_link", { postId, index }),
};

export type OverviewClient = typeof overviewClient;
