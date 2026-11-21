import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { parseMixerReply, parseOrbState, type ChannelId, type MixerReply, type OrbState } from "./orbModel";
export { parseOrbPrefs, type OrbPrefsView } from "./orbModel";

export function hostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function mixerCall(command: string, args?: Record<string, unknown>): Promise<MixerReply> {
  const reply = parseMixerReply(await invoke<unknown>(command, args));
  if (!reply) throw new Error("MIXER_REPLY_INVALID");
  return reply;
}

export const mixerApi = {
  get: () => mixerCall("mixer_get"),
  level: (channel: ChannelId, percent: number) => mixerCall("mixer_set", { request: { op: "level", channel, percent } }),
  mute: (channel: ChannelId, muted: boolean) => mixerCall("mixer_set", { request: { op: "mute", channel, muted } }),
  app: (exe: string) => mixerCall("mixer_set", { request: { op: "app", exe } }),
  reset: () => mixerCall("mixer_set", { request: { op: "reset" } }),
  focus: () => mixerCall("mixer_focus_toggle"),
};

export const orbApi = {
  click: (target: "disc" | "close") => invoke<{ open: boolean }>("orb_click", { target }),
  panelHeight: (height: number) => invoke<unknown>("orb_panel_height", { height }),
  prefs: () => invoke<unknown>("orb_prefs_get"),
  setPrefs: (patch: Record<string, unknown>) => invoke<unknown>("orb_prefs_set", { patch }),
};

export async function subscribeOrbState(callback: (state: OrbState) => void): Promise<() => void> {
  if (!hostAvailable()) return () => {};
  return listen<unknown>("orb://state", (event) => callback(parseOrbState(event.payload)));
}

export async function subscribeMixer(callback: (reply: MixerReply) => void): Promise<() => void> {
  if (!hostAvailable()) return () => {};
  return listen<unknown>("orb://mixer", (event) => {
    const reply = parseMixerReply(event.payload);
    if (reply) callback(reply);
  });
}
