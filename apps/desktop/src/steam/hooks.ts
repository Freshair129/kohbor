import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, useSyncExternalStore } from "react";
import { connectSteamStatus, createDotaLauncher } from "./controller";
import type { SteamStatus } from "./model";
import { readShowSteamName, writeShowSteamName } from "./steamName";

export function useSteamStatus(): SteamStatus | null {
  const [status, setStatus] = useState<SteamStatus | null>(null);
  useEffect(() => connectSteamStatus({ get: () => invoke("steam_status"),
    subscribe: cb => listen<unknown>("steam://status", event => cb(event.payload)) }, setStatus), []);
  return status;
}
export function useShowSteamName() {
  const [show, setShow] = useState(() => { try { return readShowSteamName(window.localStorage); } catch { return true; } });
  return { show, change(value: boolean) { setShow(value); try { writeShowSteamName(window.localStorage, value); } catch { /* Storage denied. */ } } };
}
export function useDotaLauncher(status: SteamStatus | null) {
  const [controller] = useState(() => createDotaLauncher(command => invoke(command)));
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
  useEffect(() => { controller.observeStatus(); }, [controller, status]);
  return { state, controller };
}
