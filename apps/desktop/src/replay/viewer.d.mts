import type { PostMatchReplay } from "./report";
export function mountReplay(
  root: ShadowRoot,
  report: PostMatchReplay,
  options: { locale: "th" | "en"; markup: string; style: string },
): () => void;
