/**
 * Shared by the Lab HUD layout editor and the Settings overlay layout editor (UI-015): toggle the
 * browser full-screen mode on one element. Lives outside src/lab so the normal bundle carries no Lab code.
 */
export function toggleElementFullscreen(target: HTMLElement): Promise<void> {
  return document.fullscreenElement === target ? document.exitFullscreen() : target.requestFullscreen();
}

/** Leave browser full-screen mode if any element holds it; never throws. */
export function exitAnyFullscreen(): Promise<void> {
  if (typeof document === "undefined" || !document.fullscreenElement) return Promise.resolve();
  return document.exitFullscreen().catch(() => {});
}
