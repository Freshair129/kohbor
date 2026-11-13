import { Channel, invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useRef, useState } from "react";
import { liveMockRequested } from "./mockMode";
import { createCanvasPainter, createPreviewPipeline, unknownPreviewMetrics } from "./preview";
import { DEFAULT_CANVAS_FRAME } from "./canvasBox";

export function useMinimap(calibrationGeneration: number | null, enabled: boolean) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [hasFrame, setHasFrame] = useState(false);
  const [metrics, setMetrics] = useState(unknownPreviewMetrics);
  const [error, setError] = useState<string | null>(null);
  const [frameSize, setFrameSize] = useState(DEFAULT_CANVAS_FRAME);
  const frameSizeRef = useRef(DEFAULT_CANVAS_FRAME);

  useLayoutEffect(() => {
    setHasFrame(false);
    setMetrics(unknownPreviewMetrics());
    setError(null);
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (canvas && context) context.clearRect(0, 0, canvas.width, canvas.height);
    if (!enabled || calibrationGeneration === null) return;

    let disposed = false;
    let paintedOnce = false;
    let stopMock: (() => void) | undefined;
    let channel: Channel<ArrayBuffer> | null = null;
    const draw = createCanvasPainter(() => canvasRef.current);
    const pipeline = createPreviewPipeline({
      generation: calibrationGeneration,
      now: () => performance.now(),
      paint(frame) {
        const painted = draw(frame);
        if (painted && (frameSizeRef.current.width !== frame.header.width || frameSizeRef.current.height !== frame.header.height)) {
          const size = { width: frame.header.width, height: frame.header.height };
          frameSizeRef.current = size;
          setFrameSize(size);
        }
        if (painted && !paintedOnce) {
          paintedOnce = true;
          setHasFrame(true);
        }
        return painted;
      },
    });
    // Channel messages paint directly; React updates only on first paint and sampling.
    const onFrame = (buffer: ArrayBuffer) => {
      if (!disposed) pipeline.receive(buffer);
    };
    const timer = window.setInterval(() => {
      setMetrics(pipeline.sample());
      const paintError = pipeline.getError();
      if (paintError) setError(paintError);
    }, 1_000);

    if (liveMockRequested()) {
      void import("./mock").then((mock) => {
        if (!disposed) stopMock = mock.subscribeFrames(onFrame);
      }).catch((failure: unknown) => {
        if (!disposed) setError(String(failure));
      });
    } else {
      channel = new Channel<ArrayBuffer>();
      channel.onmessage = onFrame;
      void invoke<void>("minimap_subscribe", { onFrame: channel }).catch((failure: unknown) => {
        if (!disposed) setError(String(failure));
      });
    }

    return () => {
      disposed = true;
      stopMock?.();
      if (channel) channel.onmessage = () => {};
      window.clearInterval(timer);
    };
  }, [enabled, calibrationGeneration]);

  return { canvasRef, hasFrame, metrics, error, frameSize };
}
