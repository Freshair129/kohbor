// Settings for the opt-in, own-account OpenDota history import.
import { useEffect, useState } from "react";
import { overviewCopy, type OverviewLocale } from "./overviewCopy";
import { overviewClient, type OverviewClient } from "./overviewClient";

export function OpenDotaSettings({ locale, client = overviewClient }: { locale: OverviewLocale; client?: OverviewClient }) {
  const t = overviewCopy(locale).stats;
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let alive = true;
    client.openDotaPrefs()
      .then((prefs) => { if (alive) setEnabled(prefs.enabled); })
      .catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [client]);

  const change = (next: boolean) => {
    setSaving(true);
    client.setOpenDotaEnabled(next)
      .then((prefs) => { setEnabled(prefs.enabled); setFailed(false); })
      .catch(() => setFailed(true))
      .finally(() => setSaving(false));
  };

  return (
    <section className="card shell-news-settings" aria-label={t.openDotaSettingsTitle}>
      <h2 className="shell-settings-title">{t.openDotaSettingsTitle}</h2>
      <p className="muted">{t.openDotaDisclosure}</p>
      <label className="shell-developer-toggle">
        <input type="checkbox" checked={enabled === true} disabled={enabled === null || saving}
          onChange={(event) => change(event.currentTarget.checked)} />
        <span>{t.openDotaSettingsToggle}</span>
      </label>
      {failed && <p className="locale-notice" role="status">{t.openDotaSettingsUnavailable}</p>}
    </section>
  );
}
