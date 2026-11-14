// @ts-ignore Explicit extensions support Node's TypeScript loader.
import { displayHeroName } from "./heroName.ts";

export type HeroPortraitResult = { state: "none" | "loading" | "error"; src?: never } | { state: "ready"; src: string };
export type HeroPortraitInvoke = (command: "hero_portrait", args: { hero_id: string }) => Promise<unknown>;
const NONE: HeroPortraitResult = { state: "none" };
const LOADING: HeroPortraitResult = { state: "loading" };
const ERROR: HeroPortraitResult = { state: "error" };
const PNG_PREFIX = "data:image/png;base64,";
const BASE64 = /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/;

function keyFor(heroId: string | null): string | null {
  return heroId !== null && heroId.trim().length > 0 ? heroId : null;
}
function isBase64(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && BASE64.test(value);
}

export function portraitInitials(heroId: string | null): string {
  const name = displayHeroName(heroId)?.name;
  if (!name) return "—";
  return name.split(/[\s-]+/).filter(Boolean).slice(0, 2).map(word => word.charAt(0).toUpperCase()).join("") || "—";
}

/** Only host-supplied PNG bytes become an image source. No file paths or network URLs. */
export function parseHeroPortraitResponse(input: unknown): HeroPortraitResult {
  if (input === null) return NONE;
  if (!input || typeof input !== "object" || Array.isArray(input)) return ERROR;
  const value = input as Record<string, unknown>;
  return value.mime === "image/png" && isBase64(value.base64) ? { state: "ready", src: PNG_PREFIX + value.base64 } : ERROR;
}

export function deriveHeroPortraitView(heroId: string | null, result: HeroPortraitResult, imageFailed = false) {
  if (keyFor(heroId) === null) return { state: "none" as const, initials: "—", src: null };
  const initials = portraitInitials(heroId);
  if (result.state !== "ready") return { state: result.state, initials, src: null };
  const localPng = result.src.startsWith(PNG_PREFIX) && isBase64(result.src.slice(PNG_PREFIX.length));
  return imageFailed || !localPng ? { state: "error" as const, initials, src: null } : { state: "ready" as const, initials, src: result.src };
}

/** Per-renderer memory cache, including misses and failures; no automatic retry or persistence. */
export function createHeroPortraitCache(invoke: HeroPortraitInvoke) {
  const entries = new Map<string, { value: HeroPortraitResult; pending: Promise<HeroPortraitResult> }>();
  return {
    peek(heroId: string | null): HeroPortraitResult | undefined {
      const key = keyFor(heroId);
      return key === null ? NONE : entries.get(key)?.value;
    },
    load(heroId: string | null): Promise<HeroPortraitResult> {
      const key = keyFor(heroId);
      if (key === null) return Promise.resolve(NONE);
      const cached = entries.get(key);
      if (cached) return cached.pending;
      const pending = Promise.resolve()
        .then(() => invoke("hero_portrait", { hero_id: key }))
        .then(parseHeroPortraitResponse)
        .catch(() => NONE)
        .then(value => { const entry = entries.get(key); if (entry) entry.value = value; return value; });
      entries.set(key, { value: LOADING, pending });
      return pending;
    },
  };
}
