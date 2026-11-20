import { useCallback, useRef, useState } from "react";

export const RAIL_STATES = ["collapsed", "expanded"] as const;
export type RailState = (typeof RAIL_STATES)[number];
export const DEFAULT_RAIL_STATE: RailState = "collapsed";
export const RAIL_STORAGE_KEY = "kohbor.rail-left";

export function isRailState(value: unknown): value is RailState {
  return typeof value === "string" && RAIL_STATES.some(state => state === value);
}

export function nextRailState(state: RailState): RailState {
  return state === "expanded" ? "collapsed" : "expanded";
}

export function readRailState(storage: Pick<Storage, "getItem">): RailState {
  try {
    const saved = storage.getItem(RAIL_STORAGE_KEY);
    return isRailState(saved) ? saved : DEFAULT_RAIL_STATE;
  } catch {
    return DEFAULT_RAIL_STATE;
  }
}

/** Returns the requested state even if storage is unavailable, so the UI still changes in memory. */
export function persistRailState(storage: Pick<Storage, "setItem">, value: RailState): RailState {
  try { storage.setItem(RAIL_STORAGE_KEY, value); } catch { /* In-memory display still changes. */ }
  return value;
}

export function shouldToggleRailFromShortcut(
  event: Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey">,
  modalOrDrawerOpen: boolean,
): boolean {
  return !modalOrDrawerOpen && event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey
    && event.key.toLowerCase() === "b";
}

export function useRailPreference() {
  const [state, setState] = useState<RailState>(() => {
    if (typeof window === "undefined") return DEFAULT_RAIL_STATE;
    try { return readRailState(window.localStorage); } catch { return DEFAULT_RAIL_STATE; }
  });
  const stateRef = useRef(state);
  stateRef.current = state;

  const changeState = useCallback((value: RailState) => {
    if (!isRailState(value)) return;
    stateRef.current = value;
    setState(value);
    if (typeof window !== "undefined") {
      try { persistRailState(window.localStorage, value); } catch { /* In-memory display still changes. */ }
    }
  }, []);
  const toggle = useCallback(() => {
    changeState(nextRailState(stateRef.current));
  }, [changeState]);

  return { state, changeState, toggle };
}
