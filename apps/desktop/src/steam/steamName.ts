export const SHOW_STEAM_NAME_KEY = "kohbor.showSteamName";
export function readShowSteamName(storage: Pick<Storage, "getItem"> | null): boolean {
  try { return storage?.getItem(SHOW_STEAM_NAME_KEY) !== "false"; } catch { return true; }
}
export function writeShowSteamName(storage: Pick<Storage, "setItem"> | null, show: boolean): void {
  try { storage?.setItem(SHOW_STEAM_NAME_KEY, String(show)); } catch { /* Memory preference still works. */ }
}
