import { lazy, Suspense, useEffect, useState, type ReactNode } from "react";
import { AudioMuteControl, AudioPanel, useAudioMute } from "./AudioPanel";
import { copy, seconds, type Locale } from "./copy";
import { DemoPanel } from "./DemoPanel";

/** Dev-only (docs/design/KOHBOR-LAB.md): a normal release bundle drops the Lab code; a Lab test build sets VITE_KOHBOR_LAB=1 when building the frontend. */
const LabPanel = import.meta.env.DEV || import.meta.env.VITE_KOHBOR_LAB === "1" ? lazy(() => import("./lab/Lab").then((m) => ({ default: m.Lab }))) : null;
import { Deck } from "./deck/Deck";
import { GsiSetupCard } from "./shell/GsiSetupCard";
import { gsiNeedsAttention, gsiPhase } from "./shell/gsiSetupModel";
import { deckStateReasonForLocale, deriveDeckState } from "./deck/deckState";
import { Guide } from "./Guide";
import { startLiveCollection, useCaptureStatus } from "./live";
import { OwnHero } from "./OwnHero";
import { PacksPage } from "./packs/PacksPage";
import { Readiness } from "./Readiness";
import { DEFAULT_SHELL_DESTINATION, deriveHeaderState, insightsViews, statusLineModel, type InsightsView, type ShellDestination } from "./shell/state";
import { usePlayback, useRuntime } from "./runtime";
import { useSettings } from "./settings";
import { useAppVersion } from "./shell/useAppVersion";
import { deriveAppVersionView } from "./shell/appVersionModel";
import { AboutPanel } from "./shell/VersionInfo";
import { UpdateControl } from "./shell/UpdateControl";
import { useUpdateStatus } from "./shell/useUpdateStatus";
import { useDeveloperMode } from "./shell/useDeveloperMode";
import { LogsTab } from "./logs/LogsTab";
import { AccountPage } from "./account/AccountPages";
import { guestAccountPort, type AccountPort } from "./account/accountView";
import { useAccount } from "./account/useAccount";
import { VolumeToast } from "./VolumeToast";
import { useDotaLauncher, useShowSteamName, useSteamStatus } from "./steam/hooks";
import { steamPersonaName } from "./steam/model";
import { StartDotaBar } from "./steam/StartDotaBar";
import { canOpenPostMatchReplay, useMatchEnded } from "./replay/admission";
import { RailLeft } from "./shell/RailLeft";
import { RailRight } from "./shell/RailRight";
import { StatusLine } from "./shell/StatusLine";
import { StatusFooter } from "./shell/StatusFooter";
import { useResources } from "./live/useResources";
import { usePreviewMetrics } from "./live/previewMetricsStore";
import { footerStateLabel } from "./shell/statusFooterModel";
import { previewRateTooltip } from "./live/previewDiagnostics";

const ReplayPanel = import.meta.env.DEV || import.meta.env.VITE_KOHBOR_LAB === "1"
  ? lazy(() => import("./replay/ReplayPanel").then((module) => ({ default: module.ReplayPanel })))
  : null;

type ShellPage = ShellDestination | "account";

function EmptyPage({ title, note }: { title: string; note: string }) {
  return <div className="shell-screen shell-empty-state" role="status"><h2>{title}</h2><p>{note}</p></div>;
}

export function App({ devScenarioSwitcher, accountPort = guestAccountPort }: { devScenarioSwitcher?: ReactNode; accountPort?: AccountPort } = {}) {
  const { status, snapshot, hostLost, setup } = useRuntime();
  const { settings, loading, saving, notice, changeLocale } = useSettings();
  const capture = useCaptureStatus();
  const resources = useResources();
  const previewMetrics = usePreviewMetrics();
  const mute = useAudioMute(status);
  const playback = usePlayback();
  const version = deriveAppVersionView(useAppVersion());
  const update = useUpdateStatus();
  const developerMode = useDeveloperMode();
  const account = useAccount(accountPort);
  const steamStatus = useSteamStatus();
  const matchEnded = useMatchEnded(status, steamStatus);
  const steamName = useShowSteamName();
  const launch = useDotaLauncher(steamStatus);
  const [activePage, setActivePage] = useState<ShellPage>(DEFAULT_SHELL_DESTINATION);
  const [insightsView, setInsightsView] = useState<InsightsView>("summary");
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
      captureCovered: text.shell.captureCovered,
    },
    (ageMs) => seconds(ageMs, displayLocale),
  );
  const deckState = deriveDeckState({
    runtime: status,
    snapshotAvailability: snapshot?.availability ?? null,
    capture,
    hostLost,
  });
  const deckReason = deckStateReasonForLocale(deckState, displayLocale);
  const phase = status?.session.phase ?? null;
  const phaseLabel = phase === null ? "—" : text.phases[phase];
  const phaseTone = phase === "none" || phase === "draft"
    ? "unknown"
    : phase === "unsupported" || phase === "post_game"
      ? "off"
      : headerState.readiness.tone;
  const deckTone = {
    standby: "unknown",
    live: "fresh",
    novision: "warn",
    stale: "stale",
    debrief: "off",
  } as const;
  const setupNeeded = gsiNeedsAttention(gsiPhase(hostLost ? null : status ?? null));
  const intakeState = status?.intake.state ?? null;
  const gsiTone = hostLost ? "stale" : intakeState === "receiving" ? "fresh" : intakeState === "unavailable" ? "warn" : intakeState === "stale" ? "stale" : "unknown";
  const insightViewOptions = insightsViews(ReplayPanel !== null);
  const currentInsightsView = insightViewOptions.includes(insightsView) ? insightsView : "summary";
  const statusModel = statusLineModel({
    labels: { gsi: "GSI", gamePhase: text.shell.gamePhase, gameAge: text.shell.gameAge, capture: text.shell.capture, gsiSetup: text.shell.gsiSetup },
    gsi: { label: intakeState ? text.readiness.intake[intakeState] : "—", tone: gsiTone },
    phase: { label: phaseLabel, tone: phaseTone },
    gameAge: headerState.gameAge,
    capture: headerState.capture,
    deck: { label: displayLocale === "th" ? deckState.labelTh : deckState.labelEn, tone: deckTone[deckState.state], reason: deckReason },
    setupAvailable: setupNeeded,
    hostLost,
  });

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

  const navigate = (destination: ShellDestination) => setActivePage(destination);

  return (
    <div className="app" lang={displayLocale} data-theme="dark">
      {devScenarioSwitcher && <div className="shell-dev-layer">{devScenarioSwitcher}</div>}
      <RailLeft current={activePage} labels={text.shell.destinations} navigationLabel={text.shell.navigationLabel} onNavigate={navigate} />
      <main className="shell-main">
        <header className="shell-page-header">
          <div className="shell-page-heading">
            <h1 id="shell-page-heading">{activePage === "account" ? text.shell.accountProfile : text.shell.destinations[activePage]}</h1>
            {activePage === "insights" && <div className="shell-subtabs" role="tablist" aria-label={text.shell.insightsViews.label}>
              {insightViewOptions.map((view) => <button key={view} type="button" role="tab" aria-selected={currentInsightsView === view}
                aria-controls="shell-panel" onClick={() => setInsightsView(view)}>
                {text.shell.insightsViews[view]}
              </button>)}
            </div>}
          </div>
          <AudioMuteControl control={mute} locale={displayLocale} compact />
        </header>
        <StatusLine model={statusModel} label={text.shell.statusLineLabel} hostLostMessage={`${text.hostLost} — ${text.hostOutdated}`} onNavigate={navigate} />

        <section id="shell-panel" className="shell-page-content" role="region" aria-labelledby="shell-page-heading" tabIndex={-1}>
          {activePage === "overview" && <EmptyPage title={text.shell.emptyPage} note={text.shell.emptyPageNote} />}

          {activePage === "live" && (
            <div className="shell-screen shell-live-page">
              <div className="shell-live-page-top">
                <GsiSetupCard status={hostLost ? null : status} locale={displayLocale} setup={setup} />
                <StartDotaBar status={steamStatus} control={launch} showSteamName={steamName.show} locale={displayLocale} />
              </div>
              <Deck locale={displayLocale} status={status} snapshot={snapshot} state={deckState} mute={mute} playback={playback} hostLost={hostLost} steamStatus={steamStatus} gsiSetup={setup} />
            </div>
          )}

          {activePage === "packs" && <PacksPage locale={displayLocale} disabled={hostLost} phase={phase} mute={mute} />}

          {activePage === "insights" && (currentInsightsView === "replay" && ReplayPanel ? (
            <Suspense fallback={<p role="status">{text.loading}</p>}>
              <ReplayPanel locale={displayLocale} available={canOpenPostMatchReplay(status, steamStatus, hostLost, matchEnded)} />
            </Suspense>
          ) : <EmptyPage title={text.shell.emptyPage} note={text.shell.emptyPageNote} />)}

          {activePage === "store" && <EmptyPage title={text.shell.emptyPage} note={text.shell.emptyPageNote} />}

          {activePage === "settings" && (
            <div className="shell-screen shell-settings">
              <section className="card shell-locale-card" aria-label={text.languageLabel}>
                <h1 id="page-settings-heading" className="shell-settings-title">{text.shell.destinations.settings}</h1>
                <label className="locale-control">
                  <span>{text.languageLabel}</span>
                  <select className="locale-select" aria-label={text.languageLabel} value={settings?.locale ?? ""}
                    disabled={loading || !settings || saving} onChange={(event) => void changeLocale(event.currentTarget.value as Locale)}>
                    {!settings && <option value="" disabled>{text.settings.unavailable}</option>}
                    <option value="th">{text.localeOptions.th}</option>
                    <option value="en">{text.localeOptions.en}</option>
                  </select>
                </label>
                <label className="shell-developer-toggle">
                  <input type="checkbox" checked={developerMode.enabled} onChange={event => developerMode.change(event.currentTarget.checked)} />
                  <span>{text.shell.developerMode}</span>
                </label>
                <label className="shell-developer-toggle">
                  <input type="checkbox" checked={steamName.show} onChange={event => steamName.change(event.currentTarget.checked)} />
                  <span>{displayLocale === "th" ? "แสดงชื่อ Steam" : "Show Steam name"}</span>
                </label>
                {settingsNotice && <p className="locale-notice" role="status">{settingsNotice}</p>}
              </section>
              <div className="shell-guide-slot">
                {status && !hostLost ? <Guide status={status} locale={displayLocale} setup={setup} /> : (
                  <p className={hostLost ? "banner banner-warn" : "muted"} role={hostLost ? "alert" : "status"}>
                    {hostLost ? `${text.hostLost} — ${text.hostOutdated}` : text.loading}
                  </p>
                )}
              </div>
            </div>
          )}

          {activePage === "status" && (
            <div className={`shell-screen shell-status${setupNeeded ? " has-gsi-setup" : ""}${hostLost ? " is-outdated" : ""}`}>
              {setupNeeded && <div className="shell-status-setup"><GsiSetupCard status={hostLost ? null : status} locale={displayLocale} setup={setup} /></div>}
              <div className="shell-status-grid">
                <div className="shell-readiness">
                  {status ? <Readiness status={status} locale={displayLocale} /> : (
                    <p className={hostLost ? "banner banner-warn" : "muted"} role={hostLost ? "alert" : "status"}>
                      {hostLost ? `${text.hostLost} — ${text.hostOutdated}` : text.loading}
                    </p>
                  )}
                </div>
                {status && <div className="shell-own-hero"><OwnHero snap={snapshot} phase={status.session.phase}
                  sourceLive={status.intake.state === "receiving" && !hostLost} locale={displayLocale} /></div>}
                <div className="shell-audio-panel"><AudioPanel status={status} locale={displayLocale} mute={mute} /></div>
                <div className="shell-demo-panel"><DemoPanel /></div>
                {LabPanel && <Suspense fallback={null}><LabPanel /></Suspense>}
              </div>
              <AboutPanel view={version} locale={displayLocale}>
                <UpdateControl appVersion={version} locale={displayLocale} snapshot={update} check={update.check} install={update.install} openReleasePage={update.openReleasePage} />
              </AboutPanel>
            </div>
          )}

          {activePage === "logs" && (developerMode.enabled ? <LogsTab visible locale={displayLocale} /> : (
            <div className="shell-screen shell-empty-state" role="status">
              <h2>{text.shell.logsDeveloperOnly}</h2><button type="button" onClick={() => setActivePage("settings")}>{text.shell.goToSettings}</button>
            </div>
          ))}

          {activePage === "account" && <AccountPage account={account} locale={displayLocale} back={() => navigate(DEFAULT_SHELL_DESTINATION)} />}
        </section>
        <StatusFooter statusLabel={hostLost ? (displayLocale === "th" ? "ออฟไลน์" : "Offline") : footerStateLabel(capture?.phase ?? null, displayLocale)}
          tone={hostLost ? "stale" : headerState.capture.tone}
          resources={hostLost ? null : resources.current} resourceSummary={resources.summary}
          capture={hostLost ? null : capture} preview={previewMetrics}
          labels={{ region: displayLocale === "th" ? "สถานะและทรัพยากรเครื่อง" : "App status and resource metrics",
            cpu: "CPU", ram: "RAM", capture: "CAP", received: "RX", painted: "PAINT",
            cpuTitle: displayLocale === "th" ? "CPU ที่โฮสต์รายงาน" : "CPU reported by the host",
            ramTitle: displayLocale === "th" ? "หน่วยความจำกลุ่มแอปที่โฮสต์รายงาน" : "App group memory reported by the host",
            captureTitle: displayLocale === "th" ? "FPS การจับภาพที่โฮสต์รายงาน" : "Capture FPS reported by the host",
            receivedTitle: `${displayLocale === "th" ? "FPS ภาพที่หน้าต่างได้รับ" : "Frames received per second by the window"}. ${previewRateTooltip(displayLocale)}`,
            paintedTitle: `${displayLocale === "th" ? "FPS ภาพที่หน้าต่างวาด" : "Frames painted per second by the window"}. ${previewRateTooltip(displayLocale)}` }} />
      </main>
      <RailRight account={account} port={accountPort} version={version} locale={displayLocale}
        steamName={steamPersonaName(steamStatus, steamName.show)} showSteamName={steamName.show}
        labels={{ button: text.shell.accountButton, menu: text.shell.accountMenu, account: text.shell.accountProfile,
          settings: text.shell.destinations.settings, signIn: text.shell.signIn, signOut: text.shell.signOut,
          version: text.shell.version, friends: text.shell.friends, friendsNotYet: text.shell.friendsNotYet,
          presence: text.shell.presence }}
        currentPage={activePage} navigate={(page) => setActivePage(page)} />
      <VolumeToast locale={displayLocale} />
    </div>
  );
}
