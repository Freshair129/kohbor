import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import type { OwnSnapshot, RuntimeStatus } from "./types";

const POLL_MS = 1000;
const CALL_TIMEOUT_MS = 2500;
/** Consecutive failed polls after which the screen says it has lost the host. */
const LOST_AFTER_FAILURES = 3;

function withTimeout<T>(work: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const id = window.setTimeout(() => reject(new Error("timeout")), ms);
    work.then(
      (value) => {
        window.clearTimeout(id);
        resolve(value);
      },
      (error: unknown) => {
        window.clearTimeout(id);
        reject(error);
      },
    );
  });
}

export interface Runtime {
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  /**
   * True when the host has stopped answering. The last values stay on screen but
   * must be presented as not current: a silent poll failure would otherwise leave
   * an old "fresh" reading looking live.
   */
  hostLost: boolean;
  /** Runs setup. Resolves to an error code, or null on success. */
  setup: (replace: boolean) => Promise<string | null>;
}

export function useRuntime(): Runtime {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [snapshot, setSnapshot] = useState<OwnSnapshot | null>(null);
  const [failures, setFailures] = useState(0);
  const refresh = useRef<() => void>(() => {});

  useEffect(() => {
    let alive = true;
    let running = false;
    let timer: number | undefined;

    const tick = async () => {
      if (running || !alive) return;
      running = true;
      window.clearTimeout(timer);
      timer = undefined;
      try {
        const [s, o] = await withTimeout(
          Promise.all([invoke<RuntimeStatus>("runtime_get_status"), invoke<OwnSnapshot>("own_snapshot")]),
          CALL_TIMEOUT_MS,
        );
        if (alive) {
          setStatus(s);
          setSnapshot(o);
          setFailures(0);
        }
      } catch {
        // Never fabricate readiness from a failed poll; count it instead.
        if (alive) setFailures((n) => n + 1);
      } finally {
        running = false;
      }
      // A hidden window does not poll; visibilitychange resumes it.
      if (alive && !document.hidden) timer = window.setTimeout(() => void tick(), POLL_MS);
    };

    const onVisibility = () => {
      if (!document.hidden) void tick();
    };
    refresh.current = () => void tick();
    document.addEventListener("visibilitychange", onVisibility);
    void tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, []);

  const setup = useCallback(async (replace: boolean) => {
    try {
      await invoke("gsi_setup", { replace });
      return null;
    } catch (e) {
      return String(e);
    } finally {
      refresh.current();
    }
  }, []);

  return { status, snapshot, hostLost: failures >= LOST_AFTER_FAILURES, setup };
}
