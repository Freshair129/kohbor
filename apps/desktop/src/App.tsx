import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { AudioHistory } from "./AudioHistory";
import { AudioMuteControl, AudioPanel, useAudioMute } from "./AudioPanel";
import { copy, seconds, type Locale } from "./copy";
import { DemoPanel } from "./DemoPanel";
import { Guide } from "./Guide";
import { LiveTab, startLiveCollection, useCaptureStatus } from "./live";
import { OwnHero } from "./OwnHero";
import { PackPanel } from "./PackPanel";
import { Chip, Readiness } from "./Readiness";
import { nextShellTab, deriveHeaderState, SHELL_TABS, type ShellTab } from "./shell/state";
import { useRuntime } from "./runtime";
import { useSettings } from "./settings";

export function App({ devScenarioSwitcher }: { devScenarioSwitcher?: ReactNode } = {}) {
  const { status, snapshot, hostLost, setup } = useRuntime();
  const { settings, loading, saving, notice, changeLocale } = useSettings();
  const capture = useCaptureStatus();
  const mute = useAudioMute(status);
  const [activeTab, setActiveTab] = useState<ShellTab>("live");
  const tabRefs = useRef<Record<ShellTab, HTMLButtonElement | null>>({
    live: null,
    packs: null,
    settings: null,
    status: null,
  });
  const displayLocale: Locale = settings?.locale ?? "th";
  const text = copy(displayLocale);
  const headerState = deriveHeaderState(
    status,
    capture,
    hostLost,
    {
      phases: text.phases,
      staleSuffix: text.readiness.staleSuffix,
      capturePhases: text.shell.capturePhases,
      capturePaused: text.shell.capturePaused,
      captureRunning: text.shell.captureRunning,
    },
    (ageMs) => seconds(ageMs, displayLocale),
  );
  const setupNeeded = status?.intake.state === "not_configured";

  useEffect(() => {
    void startLiveCollection();
  }, []);

  useEffect(() => {
    document.documentElement.lang = displayLocale;
  }, [displayLocale]);

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

  const onTabKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, current: ShellTab) => {
    const next = nextShellTab(current, event.key);
    if (next === null) return;
    event.preventDefault();
    setActiveTab(next);
    tabRefs.current[next]?.focus();
  };

  return (
    <div className="app" lang={displayLocale}>
      {devScenarioSwitcher}
      <header className="shell-header">
        <div className="shell-brand" aria-label="Kohbor เกาะเบาะ">
          <span>Kohbor</span>
          <span lang="th">เกาะเบาะ</span>
        </div>

        <div className="shell-statuses">
          <span aria-label={`${text.readiness.title}: ${headerState.readiness.label}`}>
            <Chip tone={headerState.readiness.tone} label={headerState.readiness.label} />
          </span>
          <span className="shell-age-badge" aria-label={`${text.shell.gameAge}: ${headerState.gameAge}`}>
            <span>{text.shell.gameAge}</span>
            <b>{headerState.gameAge}</b>
          </span>
          <span aria-label={`${text.shell.capture}: ${headerState.capture.label}`}>
            <Chip tone={headerState.capture.tone} label={`${text.shell.capture} · ${headerState.capture.label}`} />
          </span>
        </div>

        {hostLost && (
          <span
            className="shell-host-lost"
            role="alert"
            title={`${text.hostLost} — ${text.hostOutdated}`}
            aria-label={`${text.hostLost} — ${text.hostOutdated}`}
          >
            {text.hostLost}
          </span>
        )}

        {setupNeeded && (
          <button type="button" className="shell-setup-hint" onClick={() => setActiveTab("settings")}>
            {text.shell.gsiSetup}
          </button>
        )}

        <AudioMuteControl control={mute} locale={displayLocale} compact />
      </header>

      <div className="shell-tab-list" role="tablist" aria-label={text.shell.navigationLabel}>
        {SHELL_TABS.map((tab) => (
          <button
            key={tab}
            ref={(element) => { tabRefs.current[tab] = element; }}
            id={`shell-tab-${tab}`}
            type="button"
            role="tab"
            aria-selected={activeTab === tab}
            aria-controls="shell-panel"
            tabIndex={activeTab === tab ? 0 : -1}
            onClick={() => setActiveTab(tab)}
            onKeyDown={(event) => onTabKeyDown(event, tab)}
          >
            {text.shell.tabs[tab]}
          </button>
        ))}
      </div>

      <section
        id="shell-panel"
        className="shell-tab-panel"
        role="tabpanel"
        aria-labelledby={`shell-tab-${activeTab}`}
        tabIndex={0}
      >
        {activeTab === "live" && (
          <div className="shell-screen shell-live">
            <LiveTab locale={displayLocale} />
          </div>
        )}

        {activeTab === "packs" && (
          <div className="shell-screen shell-packs">
            <div className={`shell-pack-panel${hostLost ? " is-outdated" : ""}`}>
              <PackPanel locale={displayLocale} disabled={hostLost} />
            </div>
            <div className={`shell-history-panel${hostLost ? " is-outdated" : ""}`}>
              <AudioHistory locale={displayLocale} disabled={hostLost} />
            </div>
          </div>
        )}

        {activeTab === "settings" && (
          <div className="shell-screen shell-settings">
            <section className="card shell-locale-card" aria-label={text.languageLabel}>
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
              {settingsNotice && <p className="locale-notice" role="status">{settingsNotice}</p>}
            </section>
            <div className="shell-guide-slot">
              {status && !hostLost ? (
                <Guide status={status} locale={displayLocale} setup={setup} />
              ) : (
                <p className={hostLost ? "banner banner-warn" : "muted"} role={hostLost ? "alert" : "status"}>
                  {hostLost ? `${text.hostLost} — ${text.hostOutdated}` : text.loading}
                </p>
              )}
            </div>
          </div>
        )}

        {activeTab === "status" && (
          <div className={`shell-screen shell-status${hostLost ? " is-outdated" : ""}`}>
            <div className="shell-status-grid">
              <div className="shell-readiness">
                {status ? <Readiness status={status} locale={displayLocale} /> : (
                  <p className={hostLost ? "banner banner-warn" : "muted"} role={hostLost ? "alert" : "status"}>
                    {hostLost ? `${text.hostLost} — ${text.hostOutdated}` : text.loading}
                  </p>
                )}
              </div>
              {status && (
                <div className="shell-own-hero">
                  <OwnHero
                    snap={snapshot}
                    phase={status.session.phase}
                    sourceLive={status.intake.state === "receiving" && !hostLost}
                    locale={displayLocale}
                  />
                </div>
              )}
              <div className="shell-audio-panel">
                <AudioPanel status={status} locale={displayLocale} mute={mute} />
              </div>
              <div className="shell-demo-panel">
                <DemoPanel />
              </div>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
