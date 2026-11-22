import type { Locale } from "./copy";

export function ObjectiveAlert({ text, locale, visible }: { text: string | null; locale: Locale; visible: boolean }) {
  if (!visible || !text) return null;
  return (
    <div className="objective-overlay-alert" role="status" aria-live="polite" lang={locale}>
      {text}
    </div>
  );
}
