// In-memory backend for the browser-only Lab preview. It is imported only by the lazy Lab runtime.
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { HUD_DEFAULT_LAYOUT, HUD_DEFAULT_LAYOUT_EVENT, HUD_LAYOUT_BOUNDS, type HudLayoutEvent } from "../hudModel.ts";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { computeHudStripHeightPercent, parseLabHudLayoutReply, type LabHudLayoutReply, type LabHudModule } from "./labHudLayout.ts";

export const MOCK_LAB_DESCRIPTION = {
  profile_version: "1.0",
  taxonomy_version: "1.0",
  source: "embedded",
  embedded_default_sha256: "00".repeat(32),
  overrides: [],
  override_path: null,
  rejected: null,
  events: {
    health_danger: { critical: true, params: {} },
    health_low: { critical: false, params: {} },
  },
};

function cloneModules(modules: readonly LabHudModule[]): LabHudModule[] {
  return modules.map((module) => ({ ...module }));
}

function reply(modules: readonly LabHudModule[], source: LabHudLayoutReply["source"]): LabHudLayoutReply {
  const parsed = parseLabHudLayoutReply({
    modules: cloneModules(modules),
    default: cloneModules(HUD_DEFAULT_LAYOUT),
    bounds: HUD_LAYOUT_BOUNDS,
    strip_height_percent: computeHudStripHeightPercent(modules),
    source,
  });
  if (parsed === null) throw new Error("MOCK_HUD_LAYOUT_INVALID");
  return parsed;
}

export interface LabHudMockOptions {
  emit: (event: string, payload: unknown) => void;
  now?: () => number;
  schedule?: (handler: () => void, milliseconds: number) => ReturnType<typeof setInterval>;
  cancel?: (timer: ReturnType<typeof setInterval>) => void;
}

export function createLabHudLayoutMock({
  emit,
  now = () => (typeof performance === "undefined" ? Date.now() : performance.now()),
  schedule = (handler, milliseconds) => setInterval(handler, milliseconds),
  cancel = (timer) => clearInterval(timer),
}: LabHudMockOptions) {
  let modules = cloneModules(HUD_DEFAULT_LAYOUT);
  let source: LabHudLayoutReply["source"] = "embedded";
  let timer: ReturnType<typeof setInterval> | null = null;

  const current = () => reply(modules, source);
  const publish = () => {
    const state = current();
    const payload: HudLayoutEvent = {
      schema: 1,
      strip_height_percent: state.strip_height_percent,
      modules: state.modules,
      objective_card: { ...HUD_DEFAULT_LAYOUT_EVENT.objective_card },
    };
    emit("hud://layout", payload);
  };

  return {
    get: () => current(),
    apply(next: readonly LabHudModule[]) {
      const validated = parseLabHudLayoutReply({
        modules: next,
        default: HUD_DEFAULT_LAYOUT,
        bounds: HUD_LAYOUT_BOUNDS,
        strip_height_percent: computeHudStripHeightPercent(next),
        source: "override",
      });
      if (validated === null) throw new Error("MOCK_HUD_LAYOUT_INVALID");
      modules = cloneModules(validated.modules);
      source = "override";
      publish();
      return current();
    },
    reset() {
      modules = cloneModules(HUD_DEFAULT_LAYOUT);
      source = "embedded";
      publish();
      return current();
    },
    preview(seconds: number) {
      if (!Number.isInteger(seconds) || seconds < 1 || seconds > 60) throw new Error("LAB_HUD_PREVIEW_SECONDS");
      publish();
      return { preview_until_ms: now() + seconds * 1000 };
    },
    publish,
    start() {
      if (timer !== null) cancel(timer);
      publish();
      timer = schedule(publish, 2000);
    },
    stop() {
      if (timer === null) return;
      cancel(timer);
      timer = null;
    },
  };
}
