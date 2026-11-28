// Card images and the minimap shipped with Kohbor (DEC-REPLAY-INTEGRATION item 3). Vite turns each
// file into an app asset URL, so the images load from the app itself (CSP img-src 'self' data:).
// The files are built from the owner's installed game by kohbor-asset-export; see provenance.json
// next to them. Rights review is required before public release.
import { bundledIcons } from "./matchBundle";

const urls = import.meta.glob<string>(
  ["../assets/dota/*/heroes/*.png", "../assets/dota/*/abilities/*.png", "../assets/dota/*/items/*.png"],
  { eager: true, query: "?url", import: "default" },
);

export const BUNDLED = bundledIcons(urls);

const replayMapUrls = import.meta.glob<string>(
  "../assets/replay-packs/*/map.png",
  { eager: true, query: "?url", import: "default" },
);

export function replayMapUrlForPatch(patch: string | null | undefined): string | null {
  if (typeof patch !== "string") return null;
  return replayMapUrls[`../assets/replay-packs/${patch}/map.png`] ?? null;
}

export function replayIconsForPatch(patch: string | null | undefined): Map<string, string> {
  const icons = new Map(BUNDLED.icons);
  icons.delete("map:");
  const map = replayMapUrlForPatch(patch);
  if (map) icons.set("map:", map);
  return icons;
}
