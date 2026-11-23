// Settings card for the Overview news feed (OVERVIEW-003): the disclosure and the off switch.
import { useEffect, useState } from "react";
import { overviewCopy, type OverviewLocale } from "./overviewCopy";
import { overviewClient, type OverviewClient } from "./overviewClient";

const FEED_URL = "https://raw.githubusercontent.com/Freshair129/kohbor-updates/main/news.json";

export function NewsSettings({ locale, client = overviewClient }: { locale: OverviewLocale; client?: OverviewClient }) {
  const t = overviewCopy(locale).news;
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    client.newsPrefs()
      .then((prefs) => { if (alive) setEnabled(prefs.enabled); })
      .catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [client]);

  const change = (next: boolean) => {
    setSaving(true);
    client.setNewsPrefs({ enabled: next, disclosure_seen: true })
      .then((prefs) => { setEnabled(prefs.enabled); setFailed(false); })
      .catch(() => setFailed(true))
      .finally(() => setSaving(false));
  };

  return (
    <section className="card shell-news-settings" aria-label={t.settingsTitle}>
      <h2 className="shell-settings-title">{t.settingsTitle}</h2>
      <label className="shell-developer-toggle">
        <input type="checkbox" checked={enabled === true} disabled={enabled === null || saving}
          onChange={(event) => change(event.currentTarget.checked)} />
        <span>{t.settingsToggle}</span>
      </label>
      <p className="muted">{t.settingsBody(FEED_URL)}</p>
      {failed && <p className="locale-notice" role="status">{t.settingsUnavailable}</p>}
    </section>
  );
}
