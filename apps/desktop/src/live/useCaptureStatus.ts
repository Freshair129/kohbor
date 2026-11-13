import { useEffect, useState } from "react";
import { parseCaptureStatus } from "./state";
import { subscribeLiveEvent } from "./events";
import { useFreshValue, type TimedValue } from "./freshness";
import type { CaptureStatus } from "./types";

export function useCaptureStatus(): CaptureStatus | null {
  const [timed, setTimed] = useState<TimedValue<CaptureStatus>>({ value: null, receivedAt: null });

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeLiveEvent("capture", (payload) => {
      if (disposed) return;
      const status = parseCaptureStatus(payload);
      setTimed({ value: status, receivedAt: performance.now() });
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {
      // A failed subscription leaves capture unknown; no prior state is treated as current.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return useFreshValue(timed);
}
