import { useCallback, useEffect, useRef, useState } from "react";
import { mediaList } from "./runtime";
import { errorCode, parseMediaListing, type MediaListing } from "./packs";

const PENDING_POLL_MS = 3_000;

export type MediaListingState = "loading" | "ok" | "unreadable" | "failed";

/** One shared media-list reader for the pack manager and the Live deck summary. */
export function useMediaListing() {
  const [listing, setListing] = useState<MediaListing | null>(null);
  const [state, setState] = useState<MediaListingState>("loading");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  const requestSequence = useRef(0);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    const sequence = ++requestSequence.current;
    setBusy(true);
    try {
      const parsed = parseMediaListing(await mediaList());
      if (!alive.current || sequence !== requestSequence.current) return;
      if (parsed) {
        setListing(parsed);
        setState("ok");
        setError(null);
      } else {
        setListing(null);
        setState("unreadable");
        setError(null);
      }
    } catch (failure) {
      if (!alive.current || sequence !== requestSequence.current) return;
      setState("failed");
      setError(errorCode(failure));
    } finally {
      if (alive.current && sequence === requestSequence.current) setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const hasPending = (listing?.pending.length ?? 0) > 0;
  useEffect(() => {
    if (!hasPending) return;
    const timer = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, PENDING_POLL_MS);
    return () => window.clearInterval(timer);
  }, [hasPending, refresh]);

  return { listing, state, error, busy, refresh, hasPending };
}
