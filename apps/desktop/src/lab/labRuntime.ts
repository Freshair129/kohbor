import { invoke } from "@tauri-apps/api/core";
import { parseDescription, parseTrace, type Description, type Trace } from "./labModel";

export interface LabEdit {
  event: string;
  name: string;
  value: number | number[];
}

function hostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** `null` when Lab is not available (release build, no KOHBOR_LAB=1, or no host). */
export async function labStatus(): Promise<Description | null> {
  if (!hostAvailable()) return null;
  try {
    const value = await invoke<unknown>("lab_status");
    if (typeof value !== "object" || value === null || (value as { available?: unknown }).available !== true) {
      return null;
    }
    return parseDescription(value);
  } catch {
    return null;
  }
}

export async function labLastTrace(): Promise<Trace | null> {
  return parseTrace(await invoke<unknown>("lab_last_trace"));
}

export async function labExport(): Promise<string> {
  return invoke<string>("lab_export");
}

export async function labApplyEdits(edits: LabEdit[]): Promise<Description> {
  const parsed = parseDescription(await invoke<unknown>("lab_apply_edits", { edits }));
  if (parsed === null) throw new Error("LAB_BAD_RESPONSE");
  return parsed;
}
