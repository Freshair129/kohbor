import { invoke } from "@tauri-apps/api/core";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { parseMinimapRecorderStatus, type MinimapRecorderStatus } from "./minimapRecorderModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { parseDescription, parseTrace, type Description, type Trace } from "./labModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { parseLayoutView, parsePreview, type BannerLayout, type LayoutView, type Preview } from "./labLayout.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { parseLabHudLayoutReply, parseLabHudPreviewReply, type LabHudLayoutReply, type LabHudModule, type LabHudPreviewReply } from "./labHudLayout.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { createLabHudLayoutMock, MOCK_LAB_DESCRIPTION } from "./labHudMock.ts";

export interface LabEdit {
  event: string;
  name: string;
  value: number | number[];
}

function hostAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function browserMockRequested(): boolean {
  return import.meta.env.DEV
    && typeof window !== "undefined"
    && new URLSearchParams(window.location.search).get("mock") === "1";
}

type LabHudMock = ReturnType<typeof createLabHudLayoutMock>;
let browserHudMock: LabHudMock | null = null;
let browserHudLayoutChannel: BroadcastChannel | null = null;

function getBrowserHudMock(): LabHudMock {
  if (browserHudMock !== null) return browserHudMock;
  browserHudMock = createLabHudLayoutMock({
    emit(event, payload) {
      if (typeof BroadcastChannel !== "undefined") {
        browserHudLayoutChannel ??= new BroadcastChannel("kohbor-mock-hud-layout");
        browserHudLayoutChannel.postMessage(payload);
      }
      if (hostAvailable()) {
        void invoke("plugin:event|emit", { event, payload }).catch(() => {});
      } else {
        window.dispatchEvent(new CustomEvent(event, { detail: payload }));
      }
    },
  });
  return browserHudMock;
}

export function stopLabHudMockEvents(): void {
  browserHudMock?.stop();
  browserHudLayoutChannel?.close();
  browserHudLayoutChannel = null;
}

function mockArgsMatch(args: Record<string, unknown> | undefined, keys: readonly string[]): args is Record<string, unknown> {
  if (args === undefined) return keys.length === 0;
  const ownKeys = Object.keys(args);
  return ownKeys.length === keys.length && keys.every((key) => Object.hasOwn(args, key));
}

function callLabHudMock(command: string, args?: Record<string, unknown>): unknown {
  const mock = getBrowserHudMock();
  if (command === "lab_hud_layout_get" && mockArgsMatch(args, [])) return mock.get();
  if (command === "lab_hud_layout_apply" && mockArgsMatch(args, ["modules"]) && Array.isArray(args.modules)) {
    return mock.apply(args.modules as LabHudModule[]);
  }
  if (command === "lab_hud_layout_reset" && mockArgsMatch(args, [])) return mock.reset();
  if (command === "lab_hud_preview" && mockArgsMatch(args, ["seconds"]) && typeof args.seconds === "number") {
    return mock.preview(args.seconds);
  }
  throw new Error(`Unsupported Lab mock command: ${command}`);
}

/** `null` when Lab is not available (release build, no KOHBOR_LAB=1, or no host). */
export async function labStatus(): Promise<Description | null> {
  if (browserMockRequested()) {
    getBrowserHudMock().start();
    return parseDescription(MOCK_LAB_DESCRIPTION);
  }
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
  if (browserMockRequested()) return null;
  return parseTrace(await invoke<unknown>("lab_last_trace"));
}

export async function labMinimapRecorderStatus(): Promise<MinimapRecorderStatus | null> {
  if (!hostAvailable()) return null;
  try {
    return parseMinimapRecorderStatus(await invoke<unknown>("lab_minimap_recorder_status"));
  } catch {
    return null;
  }
}

export async function labMinimapRecorderSet(config: {
  enabled: boolean;
  sampling_rate_fps: number;
  total_cap_bytes: number;
}): Promise<MinimapRecorderStatus> {
  const status = parseMinimapRecorderStatus(await invoke<unknown>("lab_minimap_recorder_set", {
    enabled: config.enabled,
    samplingRateFps: config.sampling_rate_fps,
    totalCapBytes: config.total_cap_bytes,
  }));
  if (status === null) throw new Error("MINIMAP_RECORDER_BAD_RESPONSE");
  return status;
}

export async function labMinimapRecorderDelete(matchHash: string): Promise<MinimapRecorderStatus> {
  const status = parseMinimapRecorderStatus(await invoke<unknown>("lab_minimap_recorder_delete", {
    matchHash,
  }));
  if (status === null) throw new Error("MINIMAP_RECORDER_BAD_RESPONSE");
  return status;
}

export async function labMinimapRecorderOpenFolder(): Promise<void> {
  await invoke<void>("lab_minimap_recorder_open_folder");
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
  if (browserMockRequested()) return null;
  const value = await invoke<unknown>("lab_reference_image");
  return typeof value === "string" ? value : null;
}

export type LabHudInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

export function createLabHudLayoutApi(call: LabHudInvoke) {
  return {
    async get(): Promise<LabHudLayoutReply> {
      const reply = parseLabHudLayoutReply(await call("lab_hud_layout_get"));
      if (reply === null) throw new Error("LAB_HUD_BAD_RESPONSE");
      return reply;
    },
    async apply(modules: readonly LabHudModule[]): Promise<LabHudLayoutReply> {
      const reply = parseLabHudLayoutReply(await call("lab_hud_layout_apply", { modules }));
      if (reply === null) throw new Error("LAB_HUD_BAD_RESPONSE");
      return reply;
    },
    async reset(): Promise<LabHudLayoutReply> {
      const reply = parseLabHudLayoutReply(await call("lab_hud_layout_reset"));
      if (reply === null) throw new Error("LAB_HUD_BAD_RESPONSE");
      return reply;
    },
    async preview(seconds: number): Promise<LabHudPreviewReply> {
      if (!Number.isInteger(seconds) || seconds < 1 || seconds > 60) throw new RangeError("LAB_HUD_PREVIEW_SECONDS");
      const reply = parseLabHudPreviewReply(await call("lab_hud_preview", { seconds }));
      if (reply === null) throw new Error("LAB_HUD_BAD_RESPONSE");
      return reply;
    },
  };
}

const labHudLayoutApi = createLabHudLayoutApi((command, args) => browserMockRequested()
  ? Promise.resolve(callLabHudMock(command, args))
  : invoke<unknown>(command, args));

export const labHudLayoutGet = () => labHudLayoutApi.get();
export const labHudLayoutApply = (modules: readonly LabHudModule[]) => labHudLayoutApi.apply(modules);
export const labHudLayoutReset = () => labHudLayoutApi.reset();
export const labHudPreview = (seconds: number) => labHudLayoutApi.preview(seconds);
