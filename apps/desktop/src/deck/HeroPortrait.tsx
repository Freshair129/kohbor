import { useState } from "react";
import { deriveHeroPortraitView } from "./heroPortraitModel";
import { useHeroPortrait } from "./useHeroPortrait";
import "./heroPortrait.css";

export function HeroPortrait({ heroId, size, alt }: { heroId: string | null; size: "sm" | "md" | "lg"; alt: string }) {
  const result = useHeroPortrait(heroId);
  const [failed, setFailed] = useState<{ heroId: string | null; src: string } | null>(null);
  const view = deriveHeroPortraitView(heroId, result, failed !== null && failed.heroId === heroId && failed.src === result.src);
  return <span className={`hero-portrait hero-portrait-${size}`} data-state={view.state}
    role={view.src ? undefined : "img"} aria-label={view.src ? undefined : alt} aria-busy={view.state === "loading"}>
    {view.src ? <img src={view.src} alt={alt} decoding="async" draggable={false}
      onError={() => setFailed({ heroId, src: view.src! })} /> : <span aria-hidden="true">{view.initials}</span>}
  </span>;
}
