import { Channel, invoke } from "@tauri-apps/api/core";
import { useLayoutEffect, useRef, useState } from "react";
import { bgraToRgba, parseFrame } from "./frame";
import { liveMockRequested } from "./mockMode";
import type { ParsedFrame } from "./types";

export function useMinimap(calibrationGeneration: number | null, enabled: boolean) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const generationRef = useRef<number | null>(calibrationGeneration);
  const latestRef = useRef<ParsedFrame | null>(null);
  const rgbaRef = useRef<Uint8ClampedArray<ArrayBuffer> | null>(null);
  const imageDataRef = useRef<ImageData | null>(null);
  const [hasFrame, setHasFrame] = useState(false);

  useLayoutEffect(() => {
    generationRef.current = enabled ? calibrationGeneration : null;
    const latest = latestRef.current;
    if (!enabled || calibrationGeneration === null || latest?.header.calibration_generation !== calibrationGeneration) {
      latestRef.current = null;
      rgbaRef.current = null;
      imageDataRef.current = null;
      setHasFrame(false);
      const canvas = canvasRef.current;
      const context = canvas?.getContext("2d");
      if (canvas && context) context.clearRect(0, 0, canvas.width, canvas.height);
    }
  }, [calibrationGeneration, enabled]);

  useLayoutEffect(() => {
    if (!enabled) return;
    let disposed = false;
    let animationFrame = 0;
    let stopMock: (() => void) | undefined;
    let channel: Channel<ArrayBuffer> | null = null;

    const drawLatest = () => {
      animationFrame = 0;
      const frame = latestRef.current;
      const canvas = canvasRef.current;
      if (!frame || !canvas || frame.header.calibration_generation !== generationRef.current) return;
      const context = canvas.getContext("2d");
      if (!context) return;

      if (canvas.width !== frame.header.width || canvas.height !== frame.header.height) {
        canvas.width = frame.header.width;
        canvas.height = frame.header.height;
        rgbaRef.current = new Uint8ClampedArray(new ArrayBuffer(frame.header.width * frame.header.height * 4));
        imageDataRef.current = new ImageData(rgbaRef.current, frame.header.width, frame.header.height);
      }
      const rgba = bgraToRgba(frame, rgbaRef.current ?? undefined);
      rgbaRef.current = rgba;
      if (!imageDataRef.current || imageDataRef.current.data.length !== rgba.length) {
        imageDataRef.current = new ImageData(rgba, frame.header.width, frame.header.height);
      } else {
        imageDataRef.current.data.set(rgba);
      }
      context.putImageData(imageDataRef.current, 0, 0);
      setHasFrame(true);
    };

    const onFrame = (buffer: ArrayBuffer) => {
      if (disposed) return;
      const frame = parseFrame(buffer);
      if (!frame || generationRef.current === null || frame.header.calibration_generation !== generationRef.current) return;
      latestRef.current = frame;
      if (!animationFrame) animationFrame = window.requestAnimationFrame(drawLatest);
    };

    if (liveMockRequested()) {
      void import("./mock").then((mock) => {
        if (disposed) return;
        stopMock = mock.subscribeFrames(onFrame);
      }).catch(() => {
        // A broken development mock does not create a production picture.
      });
    } else {
      channel = new Channel<ArrayBuffer>();
      channel.onmessage = onFrame;
      void invoke<void>("minimap_subscribe", { onFrame: channel }).catch(() => {
        // Capture transport failure leaves the map without a frame.
      });
    }

    return () => {
      disposed = true;
      stopMock?.();
      if (channel) channel.onmessage = () => {};
      if (animationFrame) window.cancelAnimationFrame(animationFrame);
      latestRef.current = null;
      rgbaRef.current = null;
      imageDataRef.current = null;
    };
  }, [enabled, calibrationGeneration]);

  return { canvasRef, hasFrame };
}
