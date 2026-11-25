import { invoke } from "@tauri-apps/api/core";
import { parsePackLoadout } from "./loadoutModel";

export async function getPackLoadout() {
  return parsePackLoadout(await invoke<unknown>("media_loadout_get"));
}

export function setPackLoadout(request: Record<string, unknown>) {
  return invoke<unknown>("media_loadout_set", { request });
}

export function cancelPackLoadout() {
  return invoke<unknown>("media_loadout_cancel");
}
