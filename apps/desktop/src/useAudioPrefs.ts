import { useEffect, useSyncExternalStore } from "react";
import { AudioPrefsStore } from "./audioPrefs";
import { audioPrefsClient } from "./runtime";

/** All renderer surfaces subscribe to this one prefs/event store. */
export const audioPrefsStore = new AudioPrefsStore(audioPrefsClient);

export function useAudioPrefs() {
  useEffect(() => {
    audioPrefsStore.start();
  }, []);
  return useSyncExternalStore(
    audioPrefsStore.subscribe,
    audioPrefsStore.getSnapshot,
    audioPrefsStore.getSnapshot,
  );
}
