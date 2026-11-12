import { useEffect, type ReactNode } from "react";
import { AudioPanel } from "./AudioPanel";
import { copy, type Locale } from "./copy";
import { Guide } from "./Guide";
import { OwnHero } from "./OwnHero";
import { Chip, Readiness } from "./Readiness";
import { useRuntime } from "./runtime";
import { useSettings } from "./settings";

export function App({ devScenarioSwitcher }: { devScenarioSwitcher?: ReactNode } = {}) {
  const { status, snapshot, hostLost, setup } = useRuntime();
  const { settings, loading, saving, notice, changeLocale } = useSettings();
  const displayLocale: Locale = settings?.locale ?? "th";
  const text = copy(displayLocale);

  useEffect(() => {
    document.documentElement.lang = displayLocale;
  }, [displayLocale]);

  const phase = status?.session.phase;
  const sourceLive = status?.intake.state === "receiving";
  const phaseTone = !phase || phase === "none" ? "unknown" : phase === "unsupported" ? "off" : sourceLive ? "fresh" : "stale";
  const phaseLabel = phase ? text.phases[phase] : "";
  const settingsNotice =
    notice === "load_failed"
      ? text.settings.loadFailed
      : notice === "conflict"
        ? text.settings.conflict
        : notice === "refresh_failed"
          ? text.settings.refreshFailed
          : notice === "save_failed"
            ? text.settings.saveFailed
            : null;

  return (
    <>
      {devScenarioSwitcher}
      <main className="app" lang={displayLocale}>
        <header className="top">
          <h1>{text.overview}</h1>
          {phase && <Chip tone={phaseTone} label={phaseTone === "stale" ? `${phaseLabel} (${text.readiness.staleSuffix})` : phaseLabel} />}
          <label className="locale-control">
            <span>{text.languageLabel}</span>
            <select
              className="locale-select"
              aria-label={text.languageLabel}
              value={settings?.locale ?? ""}
              disabled={loading || !settings || saving}
              onChange={(event) => void changeLocale(event.currentTarget.value as Locale)}
            >
              {!settings && <option value="" disabled>{text.settings.unavailable}</option>}
              <option value="th">{text.localeOptions.th}</option>
              <option value="en">{text.localeOptions.en}</option>
            </select>
          </label>
          {settingsNotice && (
            <span className="locale-notice" role="status">
              {settingsNotice}
            </span>
          )}
        </header>

        {!status ? (
          <>
            {hostLost ? (
              <p className="banner banner-warn" role="alert">
                {text.hostLost} — {text.hostLostRestart}
              </p>
            ) : (
              <p className="muted" role="status">
                {text.loading}
              </p>
            )}
            <AudioPanel status={null} locale={displayLocale} />
          </>
        ) : (
          <>
            {hostLost && (
              <p className="banner banner-warn" role="alert">
                {text.hostLost} — {text.hostOutdated}
              </p>
            )}

            <div className={hostLost ? "content is-outdated" : "content"}>
              {/* Advice derived from values that are no longer current would mislead. */}
              {!hostLost && <Guide status={status} locale={displayLocale} setup={setup} />}
              <Readiness status={status} locale={displayLocale} />
              <AudioPanel status={status} locale={displayLocale} />
              <OwnHero snap={snapshot} phase={status.session.phase} sourceLive={sourceLive} locale={displayLocale} />
            </div>
          </>
        )}
      </main>
    </>
  );
}
