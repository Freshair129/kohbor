import type { LiveLocale } from "./copy";
import { MinimapPanel } from "./MinimapPanel";

export function LiveTab({ locale = "th" }: { locale?: LiveLocale }) {
  return (
    <main className="live-tab live-dashboard" lang={locale}>
      <MinimapPanel locale={locale} />
    </main>
  );
}
