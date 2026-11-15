import { invoke } from "@tauri-apps/api/core";
import { parseDescription, parseTrace, type Description, type Trace } from "./labModel";
import { parseCatalog, type CatalogView } from "./labCatalog";
import { parseActiveBindings, parseDraft, toMappingDocument, type ActiveBindings, type DraftBinding } from "./labMapping";
import { parseLayoutView, parsePreview, type BannerLayout, type LayoutView, type Preview } from "./labLayout";

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

export async function labLayoutGet(): Promise<LayoutView> {
  const view = parseLayoutView(await invoke<unknown>("lab_layout_get"));
  if (view === null) throw new Error("LAB_BAD_RESPONSE");
  return view;
}

/** Where the real host would put the banner for `layout` on a monitor of this size. */
export async function labLayoutPreview(layout: BannerLayout, monitorW: number, monitorH: number): Promise<Preview> {
  const preview = parsePreview(await invoke<unknown>("lab_layout_preview", { layout, monitorW, monitorH }));
  if (preview === null) throw new Error("LAB_BAD_RESPONSE");
  return preview;
}

export async function labLayoutApply(layout: BannerLayout): Promise<LayoutView> {
  const view = parseLayoutView(await invoke<unknown>("lab_layout_apply", { layout }));
  if (view === null) throw new Error("LAB_BAD_RESPONSE");
  return view;
}

export async function labLayoutExport(): Promise<string> {
  return invoke<string>("lab_layout_export");
}

/** Data URL of the dev-only reference screenshot, or `null` when it is not installed. */
export async function labReferenceImage(): Promise<string | null> {
  const value = await invoke<unknown>("lab_reference_image");
  return typeof value === "string" ? value : null;
}

export async function labCatalog(): Promise<CatalogView> {
  const view = parseCatalog(await invoke<unknown>("lab_catalog"));
  if (view === null) throw new Error("LAB_BAD_RESPONSE");
  return view;
}

export async function labPackBindings(): Promise<ActiveBindings> {
  const parsed = parseActiveBindings(await invoke<unknown>("lab_pack_bindings"));
  if (parsed === null) throw new Error("LAB_BAD_RESPONSE");
  return parsed;
}

export async function labPackMappingGet(): Promise<DraftBinding[]> {
  const view = await invoke<unknown>("lab_pack_mapping_get");
  const draft = parseDraft(typeof view === "object" && view !== null ? (view as { draft?: unknown }).draft : null);
  if (draft === null) throw new Error("LAB_BAD_RESPONSE");
  return draft;
}

export async function labPackMappingApply(draft: DraftBinding[]): Promise<DraftBinding[]> {
  const view = await invoke<unknown>("lab_pack_mapping_apply", { mapping: toMappingDocument(draft) });
  const saved = parseDraft(typeof view === "object" && view !== null ? (view as { draft?: unknown }).draft : null);
  if (saved === null) throw new Error("LAB_BAD_RESPONSE");
  return saved;
}

export async function labPackMappingExport(): Promise<string> {
  return invoke<string>("lab_pack_mapping_export");
}
