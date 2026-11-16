/** Pure rules of the shared dialog, kept free of React/DOM so they are unit tested. */

/** Esc closes; every other key is left alone. */
export function dialogKeyAction(key: string, shiftKey: boolean): "close" | "tab-forward" | "tab-backward" | null {
  if (key === "Escape") return "close";
  if (key === "Tab") return shiftKey ? "tab-backward" : "tab-forward";
  return null;
}

/** Focus wrap: returns the index to focus when Tab would leave the dialog, or null to let the browser move focus. */
export function wrapFocusIndex(action: "tab-forward" | "tab-backward", activeIndex: number, count: number): number | null {
  if (count <= 0) return null;
  if (action === "tab-forward") return activeIndex >= count - 1 ? 0 : null;
  return activeIndex <= 0 ? count - 1 : null;
}

/** A click closes the dialog only when it lands on the backdrop itself, never inside the panel. */
export function isBackdropClick(target: unknown, backdrop: unknown): boolean {
  return target !== null && target === backdrop;
}
