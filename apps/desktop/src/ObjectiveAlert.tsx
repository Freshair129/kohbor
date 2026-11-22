import type { Locale } from "./copy";
import { objectiveCardContent, type ObjectiveAlertBatch, type ObjectiveId } from "./objectiveAlertModel";

function ObjectiveGlyph({ objective }: { objective: ObjectiveId }) {
  const common = { viewBox: "0 0 24 24", "aria-hidden": true as const, focusable: false as const };
  switch (objective) {
    case "water_rune":
      return <svg {...common}><path d="M12 2.7C9.1 7.1 5.5 10.8 5.5 15a6.5 6.5 0 0 0 13 0c0-4.2-3.6-7.9-6.5-12.3Z" /><path d="M9 16.2c.3 1.5 1.2 2.3 2.7 2.7" /></svg>;
    case "power_rune":
      return <svg {...common}><path d="M13.6 2.5 5.8 13h5l-.5 8.5L18.2 11h-5l.4-8.5Z" /></svg>;
    case "bounty_rune":
      return <svg {...common}><circle cx="12" cy="12" r="8.2" /><path d="m12 6.8 1.5 3.3 3.6.3-2.8 2.4.9 3.5-3.2-1.9-3.2 1.9.9-3.5-2.8-2.4 3.6-.3L12 6.8Z" /></svg>;
    case "wisdom_rune":
      return <svg {...common}><path d="m12 2.8 2.3 6.9 6.9 2.3-6.9 2.3-2.3 6.9-2.3-6.9-6.9-2.3 6.9-2.3L12 2.8Z" /><circle cx="12" cy="12" r="1.2" /></svg>;
    case "tormentor":
      return <svg {...common}><path d="m12 2.8 2.2 4 4.6-1 .1 4.7 3.1 3.3-3.4 3.1-.8 4.6-4.5-1.3-4.4 1.3-.9-4.6-3.3-3.1 3-3.3.2-4.7 4.5 1 2.1-4Z" /><circle cx="12" cy="12" r="2.1" /></svg>;
  }
}

export function ObjectiveAlert({ batch, locale, visible }: {
  batch: ObjectiveAlertBatch | null;
  locale: Locale;
  visible: boolean;
}) {
  const content = objectiveCardContent(batch, locale);
  if (!visible || !content) return null;
  return (
    <div className={`objective-card objective-card-${content.objective}`} role="status" aria-live="polite" lang={locale}>
      <span className="objective-card-glyph"><ObjectiveGlyph objective={content.objective} /></span>
      <span className="objective-card-text">{content.sentence}</span>
      {content.additionalCount > 0 ? <span className="objective-card-count">+{content.additionalCount}</span> : null}
    </div>
  );
}
