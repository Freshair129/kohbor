import { useEffect, useLayoutEffect, useReducer, useState, type ReactNode, type RefObject } from "react";
import { createRevealState, minimapPlaceholderText, reduceReveal, type MinimapState } from "./minimapStageModel";
import "./minimapStage.css";

export function MinimapStage({ source, hasFrame, covered, stageRef, children, locale, waitingReason = null }: {
  source: MinimapState; hasFrame: boolean; covered: boolean; stageRef: RefObject<HTMLDivElement | null>;
  children: ReactNode; locale: "th" | "en";
  waitingReason?: string | null;
}) {
  const [reducedMotion, setReducedMotion] = useState(() => typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
  const [machine, dispatch] = useReducer(reduceReveal, undefined, createRevealState);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReducedMotion(media.matches);
    update(); media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  useLayoutEffect(() => { dispatch({ type: "sync", source, hasFrame, reducedMotion }); }, [source, hasFrame, reducedMotion]);
  // A capture occlusion event can precede the slower Steam status event. Never style that frame as live.
  const view = covered || source === "occluded" ? "occluded" : machine.view;
  const showCover = !hasFrame || machine.view === "first_frame";
  const finish = () => dispatch({ type: "finish", revealId: machine.revealId });
  // The CSS animationend event is the normal completion; this timer is the safety net so a cancelled or
  // disabled animation can never leave the opaque cover over an already painted live minimap.
  useEffect(() => {
    if (machine.view !== "first_frame" || !machine.frameReady) return;
    const timer = window.setTimeout(
      () => dispatch({ type: "finish", revealId: machine.revealId }),
      (machine.reducedMotion ? 160 : 760) + 400,
    );
    return () => window.clearTimeout(timer);
  }, [machine.view, machine.frameReady, machine.revealId, machine.reducedMotion]);
  return <div ref={stageRef} className="live-map-stage minimap-stage" data-state={view} data-frame-ready={hasFrame}
    data-reduced-motion={machine.reducedMotion} aria-label={locale === "th" ? "แผนที่ย่อ" : "Minimap"}>
    {children}
    {showCover && <div className="minimap-cover" key={machine.revealId} onAnimationEnd={event => {
      if (event.target === event.currentTarget && event.animationName === "kb-fade-cover") finish();
    }}>
      <i className="minimap-leaf minimap-leaf-left" aria-hidden="true" onAnimationEnd={event => {
        if (event.animationName === "kb-open-left") finish();
      }} /><i className="minimap-leaf minimap-leaf-right" aria-hidden="true" />
      {!hasFrame && <div className="minimap-placeholder" role="status"><span>{minimapPlaceholderText(view, locale, waitingReason)}</span></div>}
    </div>}
  </div>;
}
