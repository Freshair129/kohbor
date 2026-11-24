import { useEffect, useRef, useState } from "react";
import type { OwnSnapshot } from "../types";
import type { LiveMatchSample } from "./pinnedPanelsModel";

function freshInteger(field: { value: number | null; availability: string } | undefined): number | null {
  return field?.availability === "fresh" && Number.isSafeInteger(field.value) ? field.value : null;
}

/** Current-match GSI samples only; this hook never persists or hydrates history from disk. */
export function useLiveMatchSamples(snapshot: OwnSnapshot | null, phase: string | null, sessionEpoch: number | null, hostLost: boolean): LiveMatchSample[] {
  const [samples, setSamples] = useState<LiveMatchSample[]>([]);
  const samplesRef = useRef<LiveMatchSample[]>([]);
  const lastSequence = useRef<number | null>(null);
  const epoch = useRef<number | null>(null);

  useEffect(() => {
    const clear = () => {
      if (samplesRef.current.length > 0) setSamples([]);
      samplesRef.current = [];
      lastSequence.current = null;
    };
    if (hostLost || phase !== "live" || sessionEpoch === null) {
      clear();
      epoch.current = sessionEpoch;
      return;
    }
    if (epoch.current !== sessionEpoch) {
      clear();
      epoch.current = sessionEpoch;
    }
    if (!snapshot || snapshot.availability !== "fresh" || (snapshot.source.age_ms ?? Infinity) > 2_000
      || snapshot.source.sequence === null || snapshot.source.sequence === undefined
      || snapshot.source.sequence === lastSequence.current) return;
    lastSequence.current = snapshot.source.sequence;
    const next: LiveMatchSample = {
      sequence: snapshot.source.sequence,
      atMs: performance.now(),
      gameSeconds: freshInteger(snapshot.map?.clock_time),
      hp: freshInteger(snapshot.own?.fields.hp),
      gpm: freshInteger(snapshot.own?.fields.gpm),
      xpm: freshInteger(snapshot.own?.fields.xpm),
      lastHits: freshInteger(snapshot.own?.fields.last_hits),
    };
    samplesRef.current = [...samplesRef.current, next];
    setSamples(samplesRef.current);
  }, [hostLost, phase, sessionEpoch, snapshot]);

  return samples;
}
