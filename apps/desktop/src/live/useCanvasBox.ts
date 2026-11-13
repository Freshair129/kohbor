import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { fitCanvasBox, type CanvasBox } from "./canvasBox";

export function useCanvasBox(frame: CanvasBox) {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const [box, setBox] = useState<CanvasBox | null>(null);
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const measure = () => {
      const next = fitCanvasBox(stage.clientWidth, stage.clientHeight, frame);
      setBox((previous) => previous?.width === next?.width && previous?.height === next?.height ? previous : next);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, [frame.width, frame.height]);

  const canvasStyle: CSSProperties = {
    aspectRatio: `${frame.width} / ${frame.height}`,
    minWidth: box?.width,
    minHeight: box?.height,
  };
  return { stageRef, canvasStyle };
}
