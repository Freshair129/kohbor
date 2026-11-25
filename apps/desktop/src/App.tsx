import { lazy, Suspense, useEffect, useRef, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { AudioMuteControl, AudioPanel, useAudioMute } from "./AudioPanel";
import { copy, seconds, type Locale } from "./copy";
import { DemoPanel } from "./DemoPanel";

/** Dev-only (docs/design/KOHBOR-LAB.md): a normal release bundle drops the Lab code; a Lab test build sets VITE_KOHBOR_LAB=1 when building the frontend. */
const LabPanel = import.meta.env.DEV || import.meta.env.VITE_KOHBOR_LAB === "1" ? lazy(() => import("./lab/Lab").then((m) => ({ default: m.Lab }))) : null;
import { Deck } from "./deck/Deck";
import { GsiSetupCard } from "./shell/GsiSetupCard";
import { GsiFirstRunDialog } from "./shell/GsiFirstRunDialog";
import { GSI_FIRST_RUN_STORAGE_KEY, gsiNeedsAttention, gsiPhase, readGsiFirstRunAnswer, shouldShowGsiFirstRun, writeGsiFirstRunAnswer } from "./shell/gsiSetupModel";
import { deckStateReasonForLocale, deriveDeckState } from "./deck/deckState";
import { Guide } from "./Guide";
import { startLiveCollection, useCaptureStatus } from "./live";
import { MinimapPanel } from "./live/MinimapPanel";
import { OwnHero } from "./OwnHero";
import { OverviewPage } from "./overview/OverviewPage";
import { NewsSettings } from "./overview/NewsSettings";
import { OpenDotaSettings } from "./overview/OpenDotaSettings";
import { PacksPage } from "./packs/PacksPage";
import { StorePage } from "./store/StorePage";
import { Readiness } from "./Readiness";
import { DEFAULT_SHELL_DESTINATION, deriveHeaderState, insightsViews, statusLineModel, type InsightsView, type ShellDestination } from "./shell/state";
import { publishHudLocale, usePlayback, useRuntime } from "./runtime";
import { useSettings } from "./settings";
import { useGlassPreference, type GlassLevel } from "./shell/glassPreference";
import { shouldToggleRailFromShortcut, useRailPreference } from "./shell/railPreference";
import { useAppVersion } from "./shell/useAppVersion";
import { deriveAppVersionView } from "./shell/appVersionModel";
import { AboutPanel } from "./shell/VersionInfo";
import { UpdateControl } from "./shell/UpdateControl";
import { useUpdateStatus } from "./shell/useUpdateStatus";
import { useDeveloperMode } from "./shell/useDeveloperMode";
import { LogsTab } from "./logs/LogsTab";
import { AccountPage } from "./account/AccountPages";
import { type AccountPort } from "./account/accountView";
import { accountPortForEnvironment } from "./account/tauriAccountPort";
import { useAccount } from "./account/useAccount";
import { VolumeToast } from "./VolumeToast";
import { OrbSettings } from "./orb/OrbSettings";
import { DraftSettings } from "./draft/DraftSettings";
import { ObjectiveSettings } from "./ObjectiveSettings";
import { ShowHotkeySettings } from "./ShowHotkeySettings";
import { LayoutEditor } from "./layoutEditor/LayoutEditor";
import { useDotaLauncher, useShowSteamName, useSteamStatus } from "./steam/hooks";
import { steamPersonaName } from "./steam/model";
import { StartDotaBar } from "./steam/StartDotaBar";
import { canOpenPostMatchReplay, useMatchEnded } from "./replay/admission";
import { AnalyticsPage } from "./analytics/AnalyticsPage";
import { returnToAnalytics as analyticsReturnContext, type AnalyticsReplayDetailsRoute, type AnalyticsReturnContext } from "./analytics/analyticsModel";
import { AnalyticsSettings } from "./analytics/AnalyticsSettings";
import { RailLeft } from "./shell/RailLeft";
import { RailRight } from "./shell/RailRight";
import { useSecondMonitorSide } from "./live/useSecondMonitorSide";
import { SecondMonitorToggle } from "./live/SecondMonitorToggle";
import { isMatchRunning, pinnedSecondMonitorSide } from "./live/secondMonitorModel";
import { StatusLine } from "./shell/StatusLine";
import { StatusFooter } from "./shell/StatusFooter";
import { useResources } from "./live/useResources";
import { usePreviewMetrics } from "./live/previewMetricsStore";
import { footerStateLabel } from "./shell/statusFooterModel";
import { previewRateTooltip } from "./live/previewDiagnostics";

const ReplayPanel = import.meta.env.DEV || import.meta.env.VITE_KOHBOR_LAB === "1"
  ? lazy(() => import("./replay/ReplayPanel").then((module) => ({ default: module.ReplayPanel })))
  : null;
const AnalyticsDetailsPanel = lazy(() => import("./replay/ReplayPanel").then((module) => ({ default: module.ReplayPanel })));

type ShellPage = ShellDestination | "account";

function invokeAccountCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

const defaultAccountPort = accountPortForEnvironment(isTauri(), invokeAccountCommand);

function EmptyPage({ title, note }: { title: string; note: string }) {
  return <div className="shell-screen shell-empty-state" role="status"><h2>{title}</h2><p>{note}</p></div>;
}

export function App({ devScenarioSwitcher, accountPort = defaultAccountPort }: { devScenarioSwitcher?: ReactNode; accountPort?: AccountPort } = {}) {
  const { status, snapshot, hostLost, setup } = useRuntime();
  const { settings, loading, saving, notice, changeLocale } = useSettings();
  const glass = useGlassPreference();
  const rail = useRailPreference();
  const secondMonitor = useSecondMonitorSide();
  const capture = useCaptureStatus();
  const resources = useResources();
  const previewMetrics = usePreviewMetrics();
  const mute = useAudioMute(status);
  const playback = usePlayback();
  const version = deriveAppVersionView(useAppVersion());
  const update = useUpdateStatus();
  const developerMode = useDeveloperMode();
  const account = useAccount(accountPort);
  useEffect(() => {
    void accountPort.refreshStatus?.();
  }, [accountPort]);
  const steamStatus = useSteamStatus();
  const matchEnded = useMatchEnded(status, steamStatus);
  const steamName = useShowSteamName();
  const launch = useDotaLauncher(steamStatus);
  const [activePage, setActivePage] = useState<ShellPage>(DEFAULT_SHELL_DESTINATION);
  const [settingsEntryPoint, setSettingsEntryPoint] = useState<"hero-card" | null>(null);
  const [insightsView, setInsightsView] = useState<InsightsView>("summary");
  const [analyticsDetail, setAnalyticsDetail] = useState<AnalyticsReplayDetailsRoute | null>(null);
  const [analyticsRestore, setAnalyticsRestore] = useState<AnalyticsReturnContext | null>(null);
  const [showGsiFirstRun, setShowGsiFirstRun] = useState(false);
  const firstRunDecisionMade = useRef(false);
  const firstRunEligible = useRef(false);
  const firstRunShownThisRun = useRef(false);
  const latestStatus = useRef(status);
  const latestHostLost = useRef(hostLost);
  latestStatus.current = status;
  latestHostLost.current = hostLost;
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
  const accountHandoffBlocked = status === null ? null : ["draft", "pregame", "live"].includes(status.session.phase);
  useEffect(() => {
    if (accountHandoffBlocked !== null) accountPort.setLiveMatch?.(accountHandoffBlocked);
  }, [accountPort, accountHandoffBlocked]);
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
  const insightViewOptions = insightsViews(ReplayPanel !== null || analyticsDetail !== null);
  const currentInsightsView = insightViewOptions.includes(insightsView) ? insightsView : "summary";
  const statusModel = statusLineModel({
    labels: {
      gsi: "GSI",
      gamePhase: text.shell.gamePhase,
      gameAge: text.shell.gameAge,
      capture: text.shell.capture,
      minimapRecording: displayLocale === "th" ? "เปิดบันทึกภาพมินิแมป (Lab)" : "Minimap recording enabled (Lab)",
      gsiSetup: text.shell.gsiSetup,
    },
    gsi: { label: intakeState ? text.readiness.intake[intakeState] : "—", tone: gsiTone },
    phase: { label: phaseLabel, tone: phaseTone },
    gameAge: headerState.gameAge,
    capture: headerState.capture,
    minimapRecording: capture?.minimap_recording === true,
    deck: { label: displayLocale === "th" ? deckState.labelTh : deckState.labelEn, tone: deckTone[deckState.state], reason: deckReason },
    setupAvailable: setupNeeded,
    hostLost,
  });

  useEffect(() => {
    void startLiveCollection();
  }, []);

  useEffect(() => {
    document.documentElement.lang = displayLocale;
    void publishHudLocale(displayLocale);
  }, [displayLocale]);

  useEffect(() => {
    if (status === null || hostLost) return;
    if (!firstRunDecisionMade.current) {
      firstRunDecisionMade.current = true;
      const answer = readGsiFirstRunAnswer(() => window.localStorage.getItem(GSI_FIRST_RUN_STORAGE_KEY));
      firstRunEligible.current = shouldShowGsiFirstRun(gsiPhase(status), answer, firstRunShownThisRun.current);
    }
    if (!firstRunEligible.current) return;

    let observer: MutationObserver | null = null;
    const offerWhenAvailable = () => {
      if (latestHostLost.current || gsiPhase(latestStatus.current) !== "not_installed") {
        observer?.disconnect();
        return;
      }
      const otherModalOpen = document.querySelector('dialog[open], [aria-modal="true"]') !== null;
      if (otherModalOpen) return;
      if (firstRunShownThisRun.current) return;
      firstRunShownThisRun.current = true;
      setShowGsiFirstRun(true);
      observer?.disconnect();
    };
    offerWhenAvailable();
    if (!firstRunShownThisRun.current) {
      observer = new MutationObserver(offerWhenAvailable);
      observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, attributeFilter: ["open", "aria-modal"] });
    }
    return () => observer?.disconnect();
  }, [status !== null, hostLost]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modalOrDrawerOpen = document.querySelector('dialog[open], [aria-modal="true"]') !== null;
      if (!shouldToggleRailFromShortcut(event, modalOrDrawerOpen)) return;
      event.preventDefault();
      rail.toggle();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [rail.toggle]);

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

  const navigate = (destination: ShellDestination) => {
    setSettingsEntryPoint(null);
    setActivePage(destination);
  };
  const openHeroCardSettings = () => {
    setSettingsEntryPoint("hero-card");
    setActivePage("settings");
  };
  const openAnalyticsDetails = (route: AnalyticsReplayDetailsRoute) => {
    setAnalyticsRestore(null);
    setAnalyticsDetail(route);
    setInsightsView("replay");
  };
  const returnToAnalyticsPage = () => {
    if (analyticsDetail) setAnalyticsRestore(analyticsReturnContext(analyticsDetail));
    setAnalyticsDetail(null);
    setInsightsView("summary");
  };
  const [postGameMatchKey, setPostGameMatchKey] = useState<{ key: string; nonce: number } | null>(null);
  // The post-game button opens Insights -> Analytics on the match that just ended. The panel knows the match only by
  // its install-keyed history hash; AnalyticsPage resolves that hash against the replay library list.
  const openPostGameReplay = (matchKey?: string) => {
    setAnalyticsDetail(null);
    setAnalyticsRestore(null);
    setActivePage("insights");
    if (matchKey) {
      setPostGameMatchKey(previous => ({ key: matchKey, nonce: (previous?.nonce ?? 0) + 1 }));
      setInsightsView("summary");
    } else {
      setInsightsView("replay");
    }
  };
  // Second-monitor mode (UI-016): on the Live page only while a match runs (hero selection through the game) and a side
  // is set; otherwise the centred deck, as with "off" (owner report on build 3815c73, 2026-11-23). The deck itself shows
  // "window too small" below the smallest supported size instead of a broken or scrolling layout.
  const inMatch = isMatchRunning(status?.session.phase ?? null, hostLost);
  const pinnedSide = pinnedSecondMonitorSide({ side: secondMonitor.side, onLivePage: activePage === "live", phase: status?.session.phase ?? null, hostLost });

  return (
    <div className="app glass-field" lang={displayLocale} data-theme="dark" data-glass={glass.level} data-rail={rail.state} data-page={activePage}
      data-live-monitor={pinnedSide ?? undefined} data-match={pinnedSide && inMatch ? "true" : undefined}
      data-live-match={activePage === "live" && inMatch ? "true" : undefined}>
      {devScenarioSwitcher && <div className="shell-dev-layer">{devScenarioSwitcher}</div>}
      <RailLeft current={activePage} labels={text.shell.destinations} navigationLabel={text.shell.navigationLabel}
        gameConnection={steamStatus?.gsi ?? "unknown"} locale={displayLocale} railState={rail.state} onToggleRail={rail.toggle} onNavigate={navigate} />
      <main className="shell-main">
        <header className="shell-page-header" data-analytics-layout={activePage === "insights" && currentInsightsView === "summary" ? "true" : undefined}>
          <div className="shell-page-heading">
            <div className="shell-page-title">
              <p className="shell-page-eyebrow">DOTA 2</p>
              <h1 id="shell-page-heading">{activePage === "account" ? text.shell.accountProfile : text.shell.destinations[activePage]}</h1>
            </div>
            {activePage === "insights" && <div className="shell-subtabs" role="tablist" aria-label={text.shell.insightsViews.label}>
              {insightViewOptions.map((view) => <button key={view} type="button" role="tab" aria-selected={currentInsightsView === view}
                aria-controls="shell-panel" onClick={() => setInsightsView(view)}>
                {text.shell.insightsViews[view]}
              </button>)}
            </div>}
          </div>
          <div className="shell-page-head-tools">
            {activePage === "insights" && currentInsightsView === "summary" && <div className="analytics-shell-tools" data-analytics-shell-tools />}
            {/* Owner decision 2026-11-23 item 5: no GSI/game status chips on the Overview; every other page keeps them. */}
            {activePage !== "overview" && <StatusLine model={statusModel} label={text.shell.statusLineLabel} hostLostMessage={`${text.hostLost} — ${text.hostOutdated}`} onNavigate={navigate} />}
            {activePage === "live" && steamStatus?.dota_running === false && !steamStatus.launch_pending && <div className="shell-live-start-control">
              <StartDotaBar status={steamStatus} control={launch} showSteamName={steamName.show} locale={displayLocale} />
            </div>}
            {activePage === "live" && <SecondMonitorToggle side={secondMonitor.side} onChange={secondMonitor.change} locale={displayLocale === "en" ? "en" : "th"} />}
            <AudioMuteControl control={mute} locale={displayLocale} compact />
          </div>
        </header>

        <section id="shell-panel" className="shell-page-content" role="region" aria-labelledby="shell-page-heading" tabIndex={-1}>
          {activePage === "overview" && <OverviewPage locale={displayLocale} />}

          {activePage === "live" && (
            <div className="shell-screen shell-live-page">
              <Deck locale={displayLocale} status={status} snapshot={snapshot} state={deckState} hostLost={hostLost} steamStatus={steamStatus}
                inMatch={inMatch} matchEnded={matchEnded} secondMonitor={pinnedSide ? { side: pinnedSide } : undefined}
                onOpenReplay={openPostGameReplay} />
            </div>
          )}

          {activePage === "packs" && <PacksPage locale={displayLocale} disabled={hostLost}
            phase={phase} mute={mute} storeOnly={false} onStoreBack={() => setActivePage("packs")}
            onStoreOpen={() => setActivePage("store")} />}

          {activePage === "store" && (
            developerMode.enabled ? (
              <StorePage locale={displayLocale} disabled={hostLost} phase={phase} onBackToPacks={() => setActivePage("packs")} />
            ) : (
              <PacksPage locale={displayLocale} disabled={hostLost} phase={phase} mute={mute} storeOnly={true} developerMode={false} onStoreBack={() => setActivePage("packs")} />
            )
          )}

          {activePage === "insights" && (currentInsightsView === "summary" || analyticsDetail !== null) && <div hidden={currentInsightsView === "replay"}
            style={{ width: "100%", height: "100%", minWidth: 0, minHeight: 0 }}>
            <AnalyticsPage locale={displayLocale} active={currentInsightsView === "summary"} restoreContext={analyticsRestore}
              openMatchKey={postGameMatchKey} onOpenReplay={openAnalyticsDetails} onOpenHeroCardSettings={openHeroCardSettings} />
          </div>}
          {activePage === "insights" && currentInsightsView === "replay" && analyticsDetail && <Suspense fallback={<p role="status">{text.loading}</p>}>
            <AnalyticsDetailsPanel locale={displayLocale} available={canOpenPostMatchReplay(status, steamStatus, hostLost, matchEnded)}
              detail={analyticsDetail} onBack={returnToAnalyticsPage} />
          </Suspense>}
          {activePage === "insights" && currentInsightsView === "replay" && !analyticsDetail && ReplayPanel && <Suspense fallback={<p role="status">{text.loading}</p>}>
            <ReplayPanel locale={displayLocale} available={canOpenPostMatchReplay(status, steamStatus, hostLost, matchEnded)} />
          </Suspense>}

          {activePage === "settings" && (
            <div className="shell-screen shell-settings">
              <ObjectiveSettings locale={displayLocale} initialSection={settingsEntryPoint} sections={{
                general: (
                  <section className="settings-preferences-card" aria-label={displayLocale === "th" ? "ตั้งค่าทั่วไป" : "General preferences"}>
                    <label className="settings-field-row">
                      <span className="settings-field-label">{text.languageLabel}</span>
                      <select className="locale-select" aria-label={text.languageLabel} value={settings?.locale ?? ""}
                        disabled={loading || !settings || saving} onChange={(event) => void changeLocale(event.currentTarget.value as Locale)}>
                        {!settings && <option value="" disabled>{text.settings.unavailable}</option>}
                        <option value="th">{text.localeOptions.th}</option>
                        <option value="en">{text.localeOptions.en}</option>
                      </select>
                      <small>{displayLocale === "th" ? "เปลี่ยนภาษาของหน้าตั้งค่าและส่วนติดต่อแอป" : "Changes the language used across the app."}</small>
                    </label>
                    <label className="settings-field-row">
                      <span className="settings-field-label">{displayLocale === "th" ? "ชื่อ Steam" : "Steam name"}</span>
                      <span className="settings-field-control"><input type="checkbox" checked={steamName.show} onChange={event => steamName.change(event.currentTarget.checked)} />
                        {displayLocale === "th" ? "แสดงชื่อ Steam" : "Show Steam name"}</span>
                      <small>{displayLocale === "th" ? "แสดงชื่อ Steam ในส่วนติดต่อแอปนี้" : "Show your Steam name in this app's interface."}</small>
                    </label>
                    <label className="settings-field-row">
                      <span className="settings-field-label">{displayLocale === "th" ? "รูปลักษณ์ / กระจก" : "Appearance / Glass"}</span>
                      <select aria-label={displayLocale === "th" ? "รูปลักษณ์ / กระจก" : "Appearance / Glass"}
                        value={glass.level} onChange={event => glass.changeLevel(event.currentTarget.value as GlassLevel)}>
                        <option value="off">{displayLocale === "th" ? "ปิด" : "Off"}</option>
                        <option value="low">{displayLocale === "th" ? "อ่อน" : "Low"}</option>
                        <option value="medium">{displayLocale === "th" ? "กลาง" : "Medium"}</option>
                        <option value="high">{displayLocale === "th" ? "เข้ม" : "High"}</option>
                      </select>
                      <small>{displayLocale === "th" ? "บันทึกในเครื่องสำหรับหน้าจอนี้" : "Saved locally for this renderer."}</small>
                    </label>
                    {settingsNotice && <p className="locale-notice" role="status">{settingsNotice}</p>}
                  </section>
                ),
                "overlay-layout": <LayoutEditor locale={displayLocale} />,
                "minimap-capture": (
                  <>
                    <section className="shell-capture-settings" aria-label={displayLocale === "th" ? "ตั้งค่าแผนที่ย่อและการจับภาพ" : "Minimap and capture settings"}>
                      <MinimapPanel locale={displayLocale === "en" ? "en" : "th"} layout="settings" />
                    </section>
                    <div className="shell-guide-slot">
                      {status && !hostLost ? <Guide status={status} locale={displayLocale} setup={setup} /> : (
                        <p className={hostLost ? "banner banner-warn" : "muted"} role={hostLost ? "alert" : "status"}>
                          {hostLost ? `${text.hostLost} — ${text.hostOutdated}` : text.loading}
                        </p>
                      )}
                    </div>
                  </>
                ),
                "replay-ai": <AnalyticsSettings locale={displayLocale} />,
                hotkeys: <ShowHotkeySettings locale={displayLocale} />,
                "privacy-data": <><DraftSettings /><NewsSettings locale={displayLocale} /><OpenDotaSettings locale={displayLocale} /></>,
                developer: (
                  <section className="settings-preferences-card" aria-label={displayLocale === "th" ? "ตั้งค่านักพัฒนา" : "Developer preferences"}>
                    <label className="settings-field-row">
                      <span className="settings-field-label">{text.shell.developerMode}</span>
                      <span className="settings-field-control"><input type="checkbox" checked={developerMode.enabled}
                        onChange={event => developerMode.change(event.currentTarget.checked)} />{displayLocale === "th" ? "เปิดโหมดนักพัฒนา" : "Enable developer mode"}</span>
                      <small>{displayLocale === "th" ? "ใช้แสดงบันทึกและเครื่องมือสำหรับนักพัฒนา" : "Enables developer logs and tools."}</small>
                    </label>
                  </section>
                ),
                "audio-controls": <OrbSettings locale={displayLocale} />,
              }} />
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

          {activePage === "account" && <AccountPage account={account} locale={displayLocale} port={accountPort}
            liveMatch={accountHandoffBlocked ?? undefined} back={() => navigate(DEFAULT_SHELL_DESTINATION)} />}
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
      {showGsiFirstRun && status && !hostLost && (
        <GsiFirstRunDialog
          locale={displayLocale}
          status={status}
          setup={setup}
          onDecline={() => {
            writeGsiFirstRunAnswer(() => window.localStorage.setItem(GSI_FIRST_RUN_STORAGE_KEY, "declined"));
            setShowGsiFirstRun(false);
          }}
          onClose={() => setShowGsiFirstRun(false)}
          onAccepted={() => writeGsiFirstRunAnswer(() => window.localStorage.setItem(GSI_FIRST_RUN_STORAGE_KEY, "accepted"))}
        />
      )}
    </div>
  );
}
