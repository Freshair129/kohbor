// @ts-ignore Both Node and Vite support JSON modules; do not duplicate the catalog labels.
import catalog from "../../../contracts/event-catalog.json" with { type: "json" };

const labels = new Map<string, { th: string; en: string }>(catalog.events.map((event: { id: string; labels: { th: string; en: string } }) => [event.id, event.labels]));
export function eventLabel(id: string | null, locale: "th" | "en"): string {
  return id === null ? (locale === "th" ? "เสียงพูด" : "Voice") : labels.get(id)?.[locale] ?? id;
}
