import { useEffect, useLayoutEffect, useReducer, useState, type ReactNode, type RefObject } from "react";
import { createRevealState, minimapPlaceholderText, reduceReveal, type MinimapState } from "./minimapStageModel";
import { phaseReasonText } from "./captureRecovery";
import "./minimapStage.css";

export function MinimapStage({ source, hasFrame, covered, stageRef, plate, children, locale, waitingReason = null }: {
  source: MinimapState; hasFrame: boolean; covered: boolean; stageRef: RefObject<HTMLDivElement | null>;
  plate: ReactNode; children: ReactNode; locale: "th" | "en";
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
  const showPlate = view === "unsupported" || view === "live";
  const showCover = view !== "live" && (view !== "occluded" || !hasFrame) && !(view === "unsupported" && plate);
  const finish = () => dispatch({ type: "finish", revealId: machine.revealId });
  return <div ref={stageRef} className="live-map-stage minimap-stage" data-state={view} data-frame-ready={hasFrame}
    data-reduced-motion={machine.reducedMotion} aria-label={locale === "th" ? "แผนที่ย่อ" : "Minimap"}>
    {children}
    {showCover && <div className="minimap-cover" key={machine.revealId} role="status" onAnimationEnd={event => {
      if (event.target === event.currentTarget && event.animationName === "kb-fade-cover") finish();
    }}>
      <i className="minimap-leaf minimap-leaf-left" aria-hidden="true" onAnimationEnd={event => {
        if (event.animationName === "kb-open-left") finish();
      }} /><i className="minimap-leaf minimap-leaf-right" aria-hidden="true" />
      <div className="minimap-placeholder"><span className="minimap-wait-mark" aria-hidden="true">▧</span>
        <h2>{minimapPlaceholderText(view, locale, waitingReason)}</h2>
        <p>{locale === "th" ? "ข้อมูลที่ยังไม่มีจะแสดงเป็น —" : "Unavailable data is shown as —"}</p>
      </div>
    </div>}
    {showPlate && plate}
    {view === "occluded" && <div className="live-map-covered" role="status">
      <strong>{locale === "th" ? "ถูกบัง" : "Covered"}</strong><span>{phaseReasonText("stale", "occluded", locale)}</span>
    </div>}
  </div>;
}
