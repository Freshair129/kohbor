import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { fitCanvasBox, type CanvasBox } from "./canvasBox";

export function useCanvasBox(frame: CanvasBox, fillStage = false) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const [box, setBox] = useState<CanvasBox | null>(null);
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage || fillStage) return;
    const measure = () => {
      const next = fitCanvasBox(stage.clientWidth, stage.clientHeight, frame);
      setBox((previous) => previous?.width === next?.width && previous?.height === next?.height ? previous : next);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, [fillStage, frame.width, frame.height]);

  const canvasStyle: CSSProperties = fillStage
    ? { width: "100%", height: "100%", minWidth: 0, minHeight: 0, aspectRatio: "1 / 1", objectFit: "fill" }
    : { aspectRatio: `${frame.width} / ${frame.height}`, minWidth: box?.width, minHeight: box?.height };
  return { stageRef, canvasStyle };
}
