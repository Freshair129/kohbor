// Card images and the minimap shipped with Kohbor (DEC-REPLAY-INTEGRATION item 3). Vite turns each
// file into an app asset URL, so the images load from the app itself (CSP img-src 'self' data:).
// The files are built from the owner's installed game by kohbor-asset-export; see provenance.json
// next to them. Rights review is required before public release.
import { bundledIcons } from "./matchBundle";

const urls = import.meta.glob<string>(
  ["../assets/dota/*/heroes/*.png", "../assets/dota/*/abilities/*.png", "../assets/dota/*/items/*.png", "../assets/dota/*/map.png"],
  { eager: true, query: "?url", import: "default" },
);

export const BUNDLED = bundledIcons(urls);
