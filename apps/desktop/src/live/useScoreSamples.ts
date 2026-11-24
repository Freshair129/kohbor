import { useEffect, useRef, useState } from "react";
import type { OwnSnapshot } from "../types";
import type { ScoreSample } from "./scoreboardModel";

function freshScore(value: { value: number | null; availability: string } | undefined): number | null {
  return value?.availability === "fresh" && Number.isSafeInteger(value.value) && value.value !== null && value.value >= 0
    ? value.value : null;
}

/** One in-memory score stream feeds both the scoreboard momentum bar and the pinned kill timeline. */
export function useScoreSamples(snapshot: OwnSnapshot | null, phase: string | null, sessionEpoch: number | null, hostLost: boolean, enabled = true): ScoreSample[] {
  const [samples, setSamples] = useState<ScoreSample[]>([]);
  const samplesRef = useRef<ScoreSample[]>([]);
  const lastSequence = useRef<number | null>(null);
  const epoch = useRef<number | null>(null);

  useEffect(() => {
    const clear = () => {
      if (samplesRef.current.length > 0) setSamples([]);
      samplesRef.current = [];
      lastSequence.current = null;
    };
    if (!enabled || hostLost || phase !== "live" || sessionEpoch === null) {
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
    const radiant = freshScore(snapshot.map?.radiant_score);
    const dire = freshScore(snapshot.map?.dire_score);
    const gameSeconds = freshScore(snapshot.map?.clock_time);
    if (radiant === null || dire === null || gameSeconds === null) return;
    lastSequence.current = snapshot.source.sequence;
    const next: ScoreSample = { atMs: performance.now(), gameSeconds, radiant, dire };
    const prior = samplesRef.current.at(-1);
    samplesRef.current = (prior && (radiant < prior.radiant || dire < prior.dire)
      ? [next] : [...samplesRef.current, next]);
    setSamples(samplesRef.current);
  }, [enabled, hostLost, phase, sessionEpoch, snapshot]);

  return samples;
}
