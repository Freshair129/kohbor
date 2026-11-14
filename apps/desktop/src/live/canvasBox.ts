export interface CanvasBox {
  width: number;
  height: number;
}

// Layout fallback specified by the owner; actual frame headers replace it on first paint.
export const DEFAULT_CANVAS_FRAME: CanvasBox = { width: 280, height: 280 };

export function fitCanvasBox(
  availableWidth: number,
  availableHeight: number,
  frame: CanvasBox = DEFAULT_CANVAS_FRAME,
): CanvasBox | null {
  if ([availableWidth, availableHeight, frame.width, frame.height].some((value) =>
    !Number.isFinite(value) || value <= 0)) return null;
  const scale = Math.min(availableWidth / frame.width, availableHeight / frame.height);
  return { width: frame.width * scale, height: frame.height * scale };
}
