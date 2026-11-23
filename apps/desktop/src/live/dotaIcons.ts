// One resolver for hero, ability and item icons (DEC-REPLAY-INTEGRATION item 3; design report 8.1 U5 and 8.3).
// Pure: it takes the bundled file map and never reads the disk or the network. The Vite glob that feeds it lives in
// dotaIconSet.ts so this module stays testable under Node.
//
// Bundled layout: apps/desktop/src/assets/dota/<build>/{heroes,abilities,items}/<name>.{png,webp}
//   hero    <name> = internal hero name without "npc_dota_hero_" (antimage)
//   ability <name> = internal ability name (antimage_mana_break)
//   item    <name> = internal item name without "item_" (blink); every recipe shares "recipe"
// A missing icon resolves to null. Callers then show the runtime portrait or a neutral name tile, never another icon.

export type IconKind = "hero" | "ability" | "item";
export type IconResolver = (kind: IconKind, internalName: string | null | undefined) => string | null;

const FOLDERS: Record<string, IconKind> = { heroes: "hero", abilities: "ability", items: "item" };
const NAME = /^[a-z0-9_]{1,64}$/;
const PATH = /(?:^|\/)assets\/dota\/([^/]+)\/(heroes|abilities|items)\/([^/]+)\.(png|webp)$/;

/** The file name an icon of this kind and internal name is stored under, or null when the name is not an internal id. */
export function iconFileName(kind: IconKind, internalName: string | null | undefined): string | null {
  if (typeof internalName !== "string" || !NAME.test(internalName)) return null;
  if (kind === "hero") {
    const name = internalName.startsWith("npc_dota_hero_") ? internalName.slice("npc_dota_hero_".length) : internalName;
    return name.length > 0 ? name : null;
  }
  if (kind === "item") {
    if (internalName.startsWith("item_recipe_")) return "recipe";
    const name = internalName.startsWith("item_") ? internalName.slice("item_".length) : internalName;
    return name.length > 0 ? name : null;
  }
  return internalName;
}

/** Natural order for build folder names, so "7.41" sorts after "7.9" and "10234" after "9999". */
function compareBuild(a: string, b: string): number {
  return a.localeCompare(b, "en", { numeric: true, sensitivity: "base" });
}

/**
 * Build a resolver from a `{ path: url }` map (the shape of an eager `import.meta.glob(..., { query: "?url" })`).
 * When several game builds are bundled, the newest build folder wins for each icon; WebP wins over PNG in one build.
 */
export function createIconResolver(files: Readonly<Record<string, unknown>> | null | undefined): IconResolver {
  const best = new Map<string, { build: string; ext: string; url: string }>();
  for (const [path, url] of Object.entries(files ?? {})) {
    if (typeof url !== "string" || url.length === 0) continue;
    const match = PATH.exec(path.replace(/\\/g, "/"));
    if (!match) continue;
    const [, build, folder, name, ext] = match as unknown as [string, string, string, string, string];
    const kind = FOLDERS[folder];
    if (!kind || !NAME.test(name)) continue;
    const key = `${kind}:${name}`;
    const prior = best.get(key);
    const newer = !prior || compareBuild(build, prior.build) > 0
      || (compareBuild(build, prior.build) === 0 && ext === "webp" && prior.ext !== "webp");
    if (newer) best.set(key, { build, ext, url });
  }
  return (kind, internalName) => {
    const file = iconFileName(kind, internalName);
    return file === null ? null : best.get(`${kind}:${file}`)?.url ?? null;
  };
}

/** Resolver used before any icon set is bundled: everything is unknown. */
export const noIcons: IconResolver = () => null;
