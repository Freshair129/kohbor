import { invoke } from "@tauri-apps/api/core";
import type { LogInvoke } from "./tail";

export const logHostInvoke: LogInvoke = (command, args) => invoke<unknown>(command, args);

export async function readLogDirectory(): Promise<string | null> {
  try {
    const path = await logHostInvoke("log_dir");
    return typeof path === "string" && path.trim() ? path : null;
  } catch { return null; }
}

export async function openLogDirectory(): Promise<boolean> {
  try { await logHostInvoke("log_open_folder"); return true; }
  catch { return false; }
}
