import { useEffect, useState } from "react";
import { usePlayback } from "../runtime";
import { liveCopy, type LiveLocale } from "./copy";
import { deriveAgentOrbMotion } from "./pinnedPanelsModel";
import { usePageVisibility } from "./usePageVisibility";

export function AgentOrb({ locale }: { locale: LiveLocale }) {
  const playback = usePlayback();
  const visibility = usePageVisibility();
  const [reducedMotion, setReducedMotion] = useState(() =>
    typeof window !== "undefined" && typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-reduced-motion: reduce)").matches : false);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  const motion = deriveAgentOrbMotion({
    outputLevel: null,
    playbackActive: playback.audio !== null,
    reducedMotion,
    visible: visibility === "visible",
  });
  const copy = liveCopy(locale).pinnedPanels;
  return (
    <section className="pp-panel pp-orb-panel" data-panel="orb" aria-label="เอเจนต์ (เสียง) — เร็ว ๆ นี้">
      <div className="pp-orb-visual" data-motion={motion.motion} data-animate={motion.animate} aria-hidden="true">
        <svg viewBox="0 0 120 120" focusable="false">
          <circle className="pp-orb-halo" cx="60" cy="60" r="47" />
          <circle className="pp-orb-ring" cx="60" cy="60" r="39" />
          <circle className="pp-orb-core" cx="60" cy="60" r="30" />
          <path className="pp-orb-wave" d="M28 60h9l5-11 8 23 7-16 7 11 7-17 7 10h14" />
        </svg>
      </div>
      <div className="pp-orb-copy">
        <h2>เอเจนต์ (เสียง) — เร็ว ๆ นี้</h2>
        <p>{motion.motion === "playing" ? copy.orbPlaying : copy.orbIdle}</p>
      </div>
    </section>
  );
}
