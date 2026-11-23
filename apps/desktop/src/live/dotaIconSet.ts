// Binds the bundled icon set to the one resolver in dotaIcons.ts. Vite expands the glob at build time; until a set is
// bundled under src/assets/dota/<build>/ the map is empty and every lookup returns null (runtime portrait fallback).
import { createIconResolver, type IconResolver } from "./dotaIcons";

const bundled = import.meta.glob("../assets/dota/**/*.{png,webp}", { eager: true, query: "?url", import: "default" }) as Record<string, string>;

export const dotaIcon: IconResolver = createIconResolver(bundled);
