import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import { AI_PROVIDER_COPY, copy, type Locale } from "../copy";
import {
  analyticsPaneLayout, analyticsShortcut, analyticsSizeClass, analyticsTopBarLayout, toggleCardColumns,
  defaultPanePreferences, readPanePreferences, resetPanePreferences, writePanePreferences,
  selectionPrompt, selectionQuote, type SelectionQuote as PaneSelectionQuote,
} from "./analyticsLayoutModel.mjs";
import {
  readHeroCardDisplay, writeHeroCardDisplay, type HeroCardDisplay,
} from "../shared/heroCardDisplay.mjs";
import { HeroCardDisplayControls } from "../settings/HeroCardDisplaySettings";
import { readDeveloperMode } from "../shell/developerMode";
import type { ReplayLibraryEntry, ReplayLibraryList, ReplayStatus } from "../replay/replayLibraryClient";
import { MatchViewerSurface, analyticsPlayerShadowCss } from "../replay/MatchViewPanel";
import { BUNDLED } from "../replay/bundledIcons";
import { admitLibraryBundle, type MatchBundle } from "../replay/matchBundle";
import { chooseMapCalibration, replayPackForPatch } from "../replay/replayPack.mjs";
import { ANALYTICS_TEMPLATE_QUESTIONS, answerAnalyticsTemplate, clock } from "../replay/coach";
import { actionFromMarkdownTarget, renderMarkdownSafe, renderMarkdownStreaming } from "./analyticsMarkdown";
import { createAnalyticsChartResolver } from "./analyticsChart";
import {
  analyticsAiVisibility, analyticsBundleSelectionKey, analyticsConsentRePrompt, analyticsDeleteTransition, analyticsDocumentDestination,
  analyticsReplayAction, analyticsReplaySelection, ANALYTICS_ORPHANS_COMMAND, buildChatRecap,
  analyticsDeathTeam, analyticsPlaybackKeyAction, analyticsViewState, enterAnalyticsReplayDetails,
  formatAnalyticsTime, replayRowFacts, UNKNOWN_VALUE,
  type AnalyticsReplayDetailsRoute, type AnalyticsReturnContext,
  type AnalyticsDeleteState,
  type AnalyticsChatTurn as ChatTurn,
} from "./analyticsModel";
import { ProviderControls, type ProviderPreferences } from "./ProviderControls";
import {
  AiReportTab, HeroComparisonTab, KnowledgeTabs, MatchSummaryTab,
  type KnowledgeDigest, type SavedReport,
} from "./knowledge";
import { providerErrorKind, shouldOfferSaveAsNote } from "./provider-model.mjs";
import "./analytics.css";

type Note = { id: string; title: string; ai: boolean };
type KeyStatus = { provider: string; set: boolean; last4: string | null; requires_key?: boolean };
type Usage = { input: number | null; cache_read: number | null; cache_creation: number | null; output: number | null };
type ReportDetail = SavedReport & { stale_reasons?: string[] };
type SelectionQuote = PaneSelectionQuote;
type KnowledgeSnapshot = {
  digest: KnowledgeDigest | null;
  own_hero_id?: string | null;
  notes?: Note[];
  reports?: SavedReport[];
  chat_history_estimated_tokens?: number;
  chat_history_token_budget?: number;
  chat_history_over_budget?: boolean;
  report_cost_reason?: string;
};
type CompletePayload = { request_id: string; error?: string; usage?: Usage; note_id?: string; note_fallback?: boolean; report?: ReportDetail };
type ToolCallPayload = { request_id?: unknown; tool?: unknown; status?: unknown };

function friendlyAnalyticsError(reason: unknown, locale: Locale, fallback: string) {
  const providerCopy = AI_PROVIDER_COPY[locale];
  switch (providerErrorKind(reason)) {
    case "match_active": return providerCopy.matchBlocked;
    case "local_program_unavailable": return providerCopy.localUnavailable;
    case "endpoint_invalid": return providerCopy.endpointInvalid;
    case "redirect_refused": return providerCopy.redirectRefused;
    case "consent_required": return providerCopy.consentRequired;
    case "key_required": return providerCopy.keyRequired;
    case "key_invalid": return providerCopy.keyInvalid;
    case "model_list_unavailable": return providerCopy.modelListUnavailable;
    default: return fallback;
  }
}

function toolCallLine(name: unknown, locale: Locale) {
  if (name === "get_deaths") return t(locale, "กำลังอ่านเหตุการณ์เสียชีวิตจากเครื่อง", "Reading death events on this device");
  if (name === "get_hero_card" || name === "get_hero_series") return t(locale, "กำลังอ่านข้อมูลฮีโร่จากเครื่อง", "Reading hero details on this device");
  if (name === "get_items") return t(locale, "กำลังอ่านไอเท็มจากเครื่อง", "Reading items on this device");
  if (name === "get_lane") return t(locale, "กำลังอ่านข้อมูลเลนจากเครื่อง", "Reading lane details on this device");
  if (name === "get_objectives") return t(locale, "กำลังอ่านวัตถุประสงค์จากเครื่อง", "Reading objectives on this device");
  if (name === "get_phase_stats") return t(locale, "กำลังอ่านผลงานตามช่วงจากเครื่อง", "Reading phase statistics on this device");
  if (name === "get_teamfight") return t(locale, "กำลังอ่านข้อมูลการปะทะจากเครื่อง", "Reading teamfight details on this device");
  if (name === "read_saved_analysis") return t(locale, "กำลังอ่านบทวิเคราะห์ที่บันทึกไว้", "Reading a saved analysis");
  if (name === "write_note") return t(locale, "กำลังบันทึกโน้ตในเครื่อง", "Saving a note on this device");
  return t(locale, "กำลังอ่านข้อมูลเพิ่มเติมจากเครื่อง", "Reading more local data");
}

function orphanRows(value: unknown): Record<string, unknown>[] {
  const rows = Array.isArray(value) ? value
    : value && typeof value === "object" && Array.isArray((value as Record<string, unknown>).orphans)
      ? (value as { orphans: unknown[] }).orphans : [];
  return rows.flatMap(item => typeof item === "string"
    ? [{}]
    : item && typeof item === "object" && !Array.isArray(item) ? [item as Record<string, unknown>] : []);
}

function preferenceStorage(): Storage | undefined {
  try { return window.localStorage; } catch { return undefined; }
}

function chatTurnsFromCommand(value: unknown): ChatTurn[] {
  if (!Array.isArray(value)) throw new Error("Invalid chat transcript");
  return value.flatMap(item => {
    if (!item || typeof item !== "object") return [];
    const turn = item as Record<string, unknown>;
    return (turn.role === "user" || turn.role === "assistant") && typeof turn.content === "string"
      ? [{ role: turn.role, content: turn.content }] : [];
  });
}

function appendAssistantDelta(turns: ChatTurn[], delta: string): ChatTurn[] {
  const last = turns.at(-1);
  if (last?.role === "assistant") return [...turns.slice(0, -1), { ...last, content: last.content + delta }];
  return [...turns, { role: "assistant", content: delta }];
}

const t = (locale: Locale, thai: string, english: string) => locale === "th" ? thai : english;
const SYSTEM_NOTE_ID = "สรุปแมตช์.md";
const CONSENT_TEXT_VERSION = 3;
const libraryClient = {
  list: () => invoke<ReplayLibraryList>("replay_library_list"),
  open: (matchId: string) => invoke<unknown>("replay_library_open", { matchId }),
  parse: (idOrPath: string) => invoke("replay_library_parse", { idOrPath }),
  pin: (matchId: string, pinned: boolean) => invoke("replay_library_pin", { matchId, pinned }),
  remove: (matchId: string) => invoke("replay_library_remove", { matchId }),
  pick: () => invoke<string | null>("replay_library_pick_file"),
  reparse: (matchId: string) => invoke("replay_library_reparse", { matchId }),
};

function systemNote(bundle: MatchBundle | null, locale: Locale): string {
  if (!bundle) return t(locale, "เลือกรีเพลย์ที่วิเคราะห์แล้วเพื่อดูสรุป", "Select an analysed replay to view its summary.");
  const heroes = Array.isArray(bundle.match.heroes) ? bundle.match.heroes as Record<string, unknown>[] : [];
  const deaths = Array.isArray(bundle.match.deaths) ? bundle.match.deaths as Record<string, unknown>[] : [];
  const count = (team: number) => heroes.length ? String(heroes.filter(hero => hero.team === team).length) : UNKNOWN_VALUE;
  const duration = bundle.durationS == null ? UNKNOWN_VALUE : clock(bundle.durationS);
  const title = t(locale, "# สรุปแมตช์", "# Match summary");
  const knownKillers = deaths.filter(death => typeof death.killer === "string" && heroes.some(hero => hero.id === death.killer)).length;
  const kills = deaths.length === 0 ? "0" : knownKillers ? String(knownKillers) : UNKNOWN_VALUE;
  return `${title}\n\n- ${t(locale, "ระยะเวลา", "Duration")}: ${duration}\n- ${t(locale, "แพตช์", "Patch")}: ${bundle.patch ?? UNKNOWN_VALUE}\n- Radiant: ${count(2)} ${t(locale, "ฮีโร่", "heroes")}\n- Dire: ${count(3)} ${t(locale, "ฮีโร่", "heroes")}\n- ${t(locale, "hero kills", "Hero kills")}: ${kills}\n- ${t(locale, "การตายที่บันทึกได้", "Recorded deaths")}: ${deaths.length}\n\n${t(locale, "เอกสารระบบนี้สร้างจากข้อมูลรีเพลย์ในเครื่อง ไม่มีการใช้โมเดลภาษา", "This system note is generated locally from replay data without a language model.")}`;
}

function heroRows(bundle: MatchBundle | null): Record<string, unknown>[] {
  return bundle && Array.isArray(bundle.match.heroes) ? bundle.match.heroes as Record<string, unknown>[] : [];
}
function deathRows(bundle: MatchBundle | null): Record<string, unknown>[] {
  return bundle && Array.isArray(bundle.match.deaths) ? bundle.match.deaths as Record<string, unknown>[] : [];
}
function thaiDate(entry: ReplayLibraryEntry | null, locale: Locale): string {
  const seconds = entry?.local_history?.ended_at_unix_s;
  if (typeof seconds === "number" && Number.isFinite(seconds)) {
    return new Intl.DateTimeFormat(locale === "th" ? "th-TH" : "en-GB", { dateStyle: "medium", timeStyle: "short", timeZone: "Asia/Bangkok" }).format(new Date(seconds * 1000));
  }
  return entry?.match_date_utc ?? UNKNOWN_VALUE;
}
function displayHeroName(value: string | null | undefined): string {
  if (!value) return UNKNOWN_VALUE;
  return value.replace(/^npc_dota_hero_/, "").split("_").filter(Boolean)
    .map(part => part[0]!.toLocaleUpperCase() + part.slice(1)).join(" ") || UNKNOWN_VALUE;
}
function localTemplate(bundle: MatchBundle | null, question: string, ownHero: string | null, selected: ReplayLibraryEntry | null, locale: Locale): string {
  if (!bundle) return t(locale, "— ไม่ทราบ", "— unknown");
  const heroes = heroRows(bundle).flatMap(hero => typeof hero.id === "string" && typeof hero.name === "string" && typeof hero.team === "number"
    ? [{ id: hero.id, name: hero.name, team: hero.team }] : []);
  const deaths = deathRows(bundle).flatMap(death => typeof death.t === "number" && typeof death.hero === "string" && typeof death.killer === "string"
    ? [{ t: death.t, hero: death.hero, killer: death.killer }] : []);
  return answerAnalyticsTemplate({ question, locale, durationS: bundle.durationS, heroes, deaths, ownHero,
    localResult: selected?.local_history?.result ?? null });
}

function statusText(status: ReplayStatus, locale: Locale) {
  const labels: Record<ReplayStatus["state"], [string, string]> = {
    available: ["ยังไม่แยก", "Not parsed"], queued: ["รอคิว", "Queued"], parsing: ["กำลังแยก", "Parsing"],
    ready: ["พร้อมดู", "Ready to view"], needs_pack: ["ยังไม่มีข้อมูลประกอบ", "Required data unavailable"], failed: ["แยกไม่สำเร็จ", "Parsing failed"],
    evicted: ["นำข้อมูลรีเพลย์ออกแล้ว", "Replay data removed"],
  };
  const label = labels[status.state]; return locale === "th" ? label[0] : label[1];
}

function moveMenuFocus(event: ReactKeyboardEvent<HTMLDivElement>) {
  const direction = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
  if (!direction && event.key !== "Home" && event.key !== "End") return;
  const items = [...event.currentTarget.querySelectorAll<HTMLElement>("[role='menuitem']:not([aria-disabled='true']):not(:disabled)")]
    .filter(item => item.getClientRects().length > 0);
  if (!items.length) return;
  event.preventDefault();
  const current = items.indexOf(document.activeElement as HTMLElement);
  const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
    : current < 0 ? (direction > 0 ? 0 : items.length - 1)
      : (current + direction + items.length) % items.length;
  items[next]!.focus();
}

export function AnalyticsPage({ locale, onOpenReplay, onOpenHeroCardSettings, restoreContext, openMatchKey = null, active = true }: {
  locale: Locale;
  onOpenReplay?: (route: AnalyticsReplayDetailsRoute) => void;
  onOpenHeroCardSettings?: () => void;
  restoreContext?: AnalyticsReturnContext | null;
  /** Open the library match whose install-keyed hash equals `key` (post-game panel). `nonce` repeats a request. */
  openMatchKey?: { key: string; nonce: number } | null;
  active?: boolean;
}) {
  const [library, setLibrary] = useState<ReplayLibraryList | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<MatchBundle | null>(null);
  const summaryMapCalibration = useMemo(() => {
    if (!bundle) return null;
    const lookup = replayPackForPatch(bundle.patch);
    if (lookup.state !== "ready") return null;
    return chooseMapCalibration(bundle.calibration, bundle.mapSize, lookup.pack.map).calibration;
  }, [bundle]);
  const [notes, setNotes] = useState<Note[]>([]);
  const [openNote, setOpenNote] = useState(SYSTEM_NOTE_ID);
  const [knowledge, setKnowledge] = useState<KnowledgeSnapshot | null>(null);
  const [knowledgeTab, setKnowledgeTab] = useState<"summary" | "compare" | "ai" | "notes">("summary");
  const [selectedReportId, setSelectedReportId] = useState<string | null>(null);
  const [loadedReport, setLoadedReport] = useState<ReportDetail | null>(null);
  const [showRawSource, setShowRawSource] = useState(false);
  const [noteDeleteConfirmId, setNoteDeleteConfirmId] = useState<string | null>(null);
  const [noteText, setNoteText] = useState("");
  const [filter, setFilter] = useState<"all" | "pinned" | "notAnalysed">("all");
  const [showList, setShowList] = useState(false);
  const [pageSize, setPageSize] = useState({ width: 0, height: 0 });
  const pageRef = useRef<HTMLDivElement>(null);
  const focusRegion = useRef(-1);
  const shellToolsRef = useRef<HTMLDivElement>(null);
  const cardSettingsTrigger = useRef<HTMLElement | null>(null);
  const moreMenuButton = useRef<HTMLButtonElement>(null);
  const layoutMenuButton = useRef<HTMLButtonElement>(null);
  const matchPickerButton = useRef<HTMLButtonElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);
  const developerMode = readDeveloperMode(window.localStorage);
  const paneSizeClass = analyticsSizeClass(pageSize.width);
  const [panePreferences, setPanePreferences] = useState(() => readPanePreferences(preferenceStorage(), "m"));
  const panePreferencesRef = useRef(panePreferences);
  const paneSizeClassRef = useRef(paneSizeClass);
  const paneWidthReady = useRef(false);
  const [shellToolsTarget, setShellToolsTarget] = useState<HTMLElement | null>(null);
  const [shellBarWidth, setShellBarWidth] = useState(0);
  const [cardSettingsOpen, setCardSettingsOpen] = useState(false);
  const [layoutMenuOpen, setLayoutMenuOpen] = useState(false);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
  const [matchPickerOpen, setMatchPickerOpen] = useState(false);
  const [heroCardDisplay, setHeroCardDisplay] = useState<HeroCardDisplay>(() => readHeroCardDisplay(preferenceStorage()));
  const [selectionAction, setSelectionAction] = useState<{ quote: SelectionQuote; rect: DOMRect } | null>(null);
  const [quoteChips, setQuoteChips] = useState<SelectionQuote[]>([]);
  const localeRef = useRef(locale);
  localeRef.current = locale;
  const [hero, setHero] = useState<string | null>(null);
  const [compareHero, setCompareHero] = useState<string | null>(null);
  const [ownHero, setOwnHero] = useState<string | null>(null);
  const [ownHeroChoice, setOwnHeroChoice] = useState("");
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ provider: "anthropic", set: false, last4: null, requires_key: true });
  const [prefs, setPrefs] = useState<ProviderPreferences>({ ai_on: false, provider: "anthropic", consent: false, model: "claude-opus-5-5", provider_endpoints: {}, provider_consents: {}, verified_providers: {}, tools_modes: {}, show_untested: false, keep_newest: 20, max_bytes: 2 * 1024 ** 3, auto_analyse: true });
  const [consentOpen, setConsentOpen] = useState(false);
  const [consentVersionNotice, setConsentVersionNotice] = useState(false);
  const notifiedConsentVersions = useRef(new Set<string>());
  const [showDataManagement, setShowDataManagement] = useState(false);
  const [deleteState, setDeleteState] = useState<AnalyticsDeleteState>("closed");
  const [dataManagementError, setDataManagementError] = useState<string | null>(null);
  const [orphans, setOrphans] = useState<Record<string, unknown>[]>([]);
  const [orphansLoading, setOrphansLoading] = useState(false);
  const [orphansError, setOrphansError] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const requestMode = useRef<"chat" | "report" | null>(null);
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [transcript, setTranscript] = useState<ChatTurn[]>([]);
  const [historyUnavailable, setHistoryUnavailable] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [toolCalls, setToolCalls] = useState<string[]>([]);
  const [createdNote, setCreatedNote] = useState<string | null>(null);
  const [noteFallback, setNoteFallback] = useState(false);
  const [loading, setLoading] = useState(true);
  const [requestId, setRequestId] = useState<string | null>(null);
  const activeRequestId = useRef<string | null>(null);
  const knowledgeRequest = useRef(0);
  const pendingEvents = useRef(new Map<string, { deltas: string[]; complete?: CompletePayload }>());
  const pendingToolCalls = useRef(new Map<string, string[]>());
  const finishRequest = useRef<(payload: CompletePayload) => void>(() => {});
  const [currentTime, setCurrentTime] = useState(0);
  const [playerSlot, setPlayerSlot] = useState<HTMLElement | null>(null);
  const [playerSpeed, setPlayerSpeed] = useState(10);
  const [playerPlaying, setPlayerPlaying] = useState(false);
  const [hoverTime, setHoverTime] = useState<{ seconds: number; percent: number } | null>(null);
  const viewerRoot = useRef<ShadowRoot | null>(null);
  const playerRange = useRef<HTMLInputElement>(null);
  const chartResolver = useMemo(() => createAnalyticsChartResolver(bundle, knowledge?.digest ?? null), [bundle, knowledge?.digest]);
  const markdown = useMemo(() => renderMarkdownSafe(noteText, { resolveChartReference: chartResolver }), [noteText, chartResolver]);
  const leftCollapsed = panePreferences.collapsed.summary;
  const rightCollapsed = panePreferences.collapsed.assistant;
  const commitPanePreferences = (next: typeof panePreferences) => {
    panePreferencesRef.current = next;
    setPanePreferences(next);
    writePanePreferences(preferenceStorage(), paneSizeClass, next);
  };
  const previewPanePreferences = (next: typeof panePreferences) => {
    panePreferencesRef.current = next;
    setPanePreferences(next);
  };
  const updatePanePreferences = (update: (current: typeof panePreferences) => typeof panePreferences) => {
    commitPanePreferences(update(panePreferencesRef.current));
  };
  const setLeftCollapsed = (collapsed: boolean) => updatePanePreferences(current => ({
    ...current, collapsed: { ...current.collapsed, summary: collapsed, ...(collapsed && current.collapsed.assistant ? { assistant: false } : {}) },
  }));
  const setRightCollapsed = (collapsed: boolean) => updatePanePreferences(current => ({
    ...current, collapsed: { ...current.collapsed, assistant: collapsed, ...(collapsed && current.collapsed.summary ? { summary: false } : {}) },
  }));
  const changeHeroCardDisplay = (next: HeroCardDisplay) => setHeroCardDisplay(writeHeroCardDisplay(preferenceStorage(), next));

  useEffect(() => {
    const target = document.querySelector<HTMLElement>("[data-analytics-shell-tools]");
    setShellToolsTarget(target);
  }, []);
  useEffect(() => {
    const node = shellToolsTarget;
    if (!node) return;
    const measure = () => setShellBarWidth(node.getBoundingClientRect().width);
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [shellToolsTarget]);
  useEffect(() => {
    if (!cardSettingsOpen && !layoutMenuOpen && !moreMenuOpen && !matchPickerOpen) return;
    const close = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (cardSettingsOpen) {
        setCardSettingsOpen(false);
        cardSettingsTrigger.current?.focus();
      } else if (matchPickerOpen) {
        setMatchPickerOpen(false);
        matchPickerButton.current?.focus();
      } else if (layoutMenuOpen) {
        setLayoutMenuOpen(false);
        layoutMenuButton.current?.focus();
      } else if (moreMenuOpen) {
        setMoreMenuOpen(false);
        moreMenuButton.current?.focus();
      }
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
  }, [cardSettingsOpen, layoutMenuOpen, moreMenuOpen, matchPickerOpen]);
  useEffect(() => {
    if (pageSize.width <= 0) return;
    const changedClass = paneSizeClassRef.current !== paneSizeClass;
    if (!changedClass && paneWidthReady.current) return;
    paneSizeClassRef.current = paneSizeClass;
    paneWidthReady.current = true;
    const next = readPanePreferences(preferenceStorage(), paneSizeClass, pageSize.width);
    panePreferencesRef.current = next;
    setPanePreferences(next);
  }, [paneSizeClass, pageSize.width]);
  useEffect(() => {
    const sync = (event: StorageEvent) => {
      if (event.key === "kohbor.analytics.panes.v1") {
        const next = readPanePreferences(preferenceStorage(), paneSizeClass, pageSize.width);
        panePreferencesRef.current = next;
        setPanePreferences(next);
      }
      if (event.key === "kohbor.hero-card.display") setHeroCardDisplay(readHeroCardDisplay(preferenceStorage()));
    };
    window.addEventListener("storage", sync);
    return () => window.removeEventListener("storage", sync);
  }, [paneSizeClass, pageSize.width]);

  const refresh = useCallback(async () => {
    try {
      const next = await libraryClient.list(); setLibrary(next);
      if (!selectedId && next.matches.length) setSelectedId(next.matches.find(row => row.status.state === "ready")?.match_id ?? next.matches[0]!.match_id);
    } catch { setError(t(locale, "อ่านคลังรีเพลย์ไม่ได้", "Could not read the replay library")); }
    finally { setLoading(false); }
  }, [locale, selectedId]);
  const refreshAi = useCallback(async () => {
    try {
      const [key, settings] = await Promise.all([
        invoke<KeyStatus>("analytics_key_status"), invoke<ProviderPreferences>("analytics_settings_get"),
      ]);
      setKeyStatus(key); setPrefs(settings);
    } catch { /* The local page stays usable when AI is unavailable. */ }
  }, []);
  const refreshKnowledge = useCallback(async (matchId: string) => {
    const request = ++knowledgeRequest.current;
    const snapshot = await invoke<KnowledgeSnapshot>("analytics_knowledge_get", { matchId });
    if (request === knowledgeRequest.current) {
      setKnowledge(snapshot);
      setNotes(snapshot.notes ?? []);
    }
    return snapshot;
  }, []);
  const refreshTranscript = useCallback(async (matchId: string) => {
    const raw = await invoke<unknown>("analytics_chat_transcript", { matchId });
    return chatTurnsFromCommand(raw);
  }, []);
  const selected = library?.matches.find(entry => entry.match_id === selectedId) ?? null;
  const selectedBundleKey = analyticsBundleSelectionKey(selected);

  useEffect(() => {
    const node = pageRef.current;
    if (!node) return;
    const measure = () => {
      const rect = node.getBoundingClientRect();
      setPageSize(current => Math.abs(current.width - rect.width) < 1 && Math.abs(current.height - rect.height) < 1
        ? current : { width: rect.width, height: rect.height });
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    let disposed = false; let unlisten: (() => void) | undefined;
    void refresh();
    void listen("replay-library://state-changed", () => { if (!disposed) void refresh(); }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [refresh]);
  useEffect(() => { void refreshAi(); }, [refreshAi]);
  useEffect(() => {
    const record = prefs.provider_consents[prefs.provider];
    const prompt = analyticsConsentRePrompt(record, CONSENT_TEXT_VERSION);
    if (!prompt.noticeKey) {
      setConsentVersionNotice(false);
      return;
    }
    const key = `${prefs.provider}:${prompt.noticeKey}`;
    if (notifiedConsentVersions.current.has(key)) return;
    notifiedConsentVersions.current.add(key);
    setConsentVersionNotice(true);
    setConsentOpen(true);
  }, [prefs.provider, prefs.provider_consents]);
  useEffect(() => {
    let alive = true;
    setTranscript([]);
    setHistoryUnavailable(false);
    setKnowledge(null);
    setNotes([]);
    knowledgeRequest.current++;
    setSelectedReportId(null);
    setLoadedReport(null);
    setKnowledgeTab("summary");
    setToolCalls([]);
    setNoteDeleteConfirmId(null);
    if (!selectedId) return;
    void refreshTranscript(selectedId).then(turns => { if (alive) setTranscript(turns); })
      .catch(() => { if (alive) setHistoryUnavailable(true); });
    void refreshKnowledge(selectedId).catch(() => {});
    return () => { alive = false; };
  }, [selectedId, refreshTranscript, refreshKnowledge]);
  useEffect(() => {
    let alive = true; setBundle(null); setNoteText(""); setOpenNote(SYSTEM_NOTE_ID);
    setShowRawSource(false); setCurrentTime(0);
    if (!selected?.match_id || selected.status.state !== "ready") return;
    const matchId = selected.match_id;
    void libraryClient.open(matchId).then(raw => {
      const result = admitLibraryBundle(raw);
      if (alive && result.ok) setBundle(result.bundle);
      else if (alive) setError(t(localeRef.current, "เปิดชุดข้อมูลรีเพลย์ไม่ได้", "Could not open the replay bundle"));
    }).catch(() => { if (alive) setError(t(localeRef.current, "รีเพลย์นี้อาจถูกนำออกจากคลังแล้ว", "This replay may have been removed from the library")); });
    return () => { alive = false; };
  }, [selectedBundleKey]);
  useEffect(() => {
    setShowRawSource(false);
    if (!selectedId) { setNoteText(systemNote(bundle, locale)); return; }
    void invoke<string>("analytics_note_read", { matchId: selectedId, noteId: openNote }).then(setNoteText)
      .catch(() => setNoteText(openNote === "สรุปแมตช์.md" ? systemNote(bundle, locale) : t(locale, "เปิดโน้ตไม่ได้ รีเพลย์อาจถูกนำออกจากคลัง แต่โน้ตยังอยู่", "Could not open the note. The replay may be removed, but notes are retained.")));
  }, [openNote, bundle, locale, selectedId]);
  useEffect(() => {
    const reportId = selectedReportId ?? knowledge?.reports?.[0]?.id ?? null;
    if (knowledgeTab !== "ai" || !selectedId || !reportId || loadedReport?.id === reportId) return;
    let alive = true;
    setLoadedReport(null);
    void invoke<Record<string, unknown>>("analytics_saved_analysis_read", { matchId: selectedId, id: reportId })
      .then(value => {
        if (!alive || value.kind !== "ai_report" || typeof value.markdown !== "string") return;
        const meta = (value.meta && typeof value.meta === "object" ? value.meta : {}) as Record<string, unknown>;
        setLoadedReport({ id: reportId, meta, markdown: value.markdown, stale_reasons: [] });
      })
      .catch(() => { if (alive) setError(t(locale, "อ่านบทวิเคราะห์ที่บันทึกไว้ไม่ได้", "Could not read the saved report")); });
    return () => { alive = false; };
  }, [knowledgeTab, selectedId, selectedReportId, knowledge?.reports, loadedReport?.id, locale]);
  finishRequest.current = payload => {
    if (payload.request_id !== activeRequestId.current) return;
    const mode = requestMode.current;
    requestMode.current = null;
    setStreaming(false); setUsage(payload.usage ?? null); setRequestId(null); activeRequestId.current = null;
    setNoteFallback(payload.note_fallback ?? false);
    if (payload.error) setError(friendlyAnalyticsError(payload.error, locale, t(locale, "คำขอไม่สำเร็จ", "The request failed")));
    if (mode === "report") {
      if (payload.report) {
        setLoadedReport(payload.report);
        setSelectedReportId(payload.report.id);
        setKnowledgeTab("ai");
      }
      if (selectedId) void refreshKnowledge(selectedId).catch(() => {});
      return;
    }
    if (payload.note_id) { setCreatedNote(payload.note_id); void refreshAi(); }
    const matchId = selectedId;
    if (matchId) void refreshTranscript(matchId).then(setTranscript).catch(() => setHistoryUnavailable(true));
  };
  useEffect(() => {
    let stopDelta: (() => void) | undefined; let stopComplete: (() => void) | undefined; let stopTool: (() => void) | undefined;
    void listen<{ request_id: string; text: string }>("analytics://delta", event => {
      if (event.payload.request_id === activeRequestId.current) {
        if (requestMode.current === "chat") {
          setAnswer(current => current + event.payload.text);
          setTranscript(current => appendAssistantDelta(current, event.payload.text));
        }
        return;
      }
      const pending = pendingEvents.current.get(event.payload.request_id) ?? { deltas: [] };
      pending.deltas.push(event.payload.text); pendingEvents.current.set(event.payload.request_id, pending);
    }).then(stop => { stopDelta = stop; }).catch(() => {});
    void listen<CompletePayload>("analytics://complete", event => {
      if (event.payload.request_id === activeRequestId.current) { finishRequest.current(event.payload); return; }
      const pending = pendingEvents.current.get(event.payload.request_id) ?? { deltas: [] };
      pending.complete = event.payload; pendingEvents.current.set(event.payload.request_id, pending);
    }).then(stop => { stopComplete = stop; }).catch(() => {});
    void listen<ToolCallPayload>("analytics://tool-activity", event => {
      const request = event.payload.request_id;
      if (typeof request !== "string" || !request || event.payload.status !== "started") return;
      const line = toolCallLine(event.payload.tool, localeRef.current);
      if (request === activeRequestId.current) setToolCalls(current => [...current, line]);
      else {
        const pending = pendingToolCalls.current.get(request) ?? [];
        pending.push(line);
        pendingToolCalls.current.set(request, pending);
      }
    }).then(stop => { stopTool = stop; }).catch(() => {});
    return () => { stopDelta?.(); stopComplete?.(); stopTool?.(); };
  }, []);

  const rows = useMemo(() => (library?.matches ?? []).filter(entry => filter === "all"
    || (filter === "pinned" && entry.pinned) || (filter === "notAnalysed" && entry.status.state !== "ready")), [library, filter]);
  useEffect(() => { setOwnHero(selected?.own_hero ?? null); setOwnHeroChoice(selected?.own_hero ?? ""); setHero(null); }, [selectedId, selected?.own_hero]);
  const aiAvailable = prefs.consent && prefs.ai_on && (!keyStatus.requires_key || keyStatus.set);
  const state = analyticsViewState({ entries: library?.matches ?? [], selectedId, selectedReady: !!bundle,
    openDocument: openNote !== SYSTEM_NOTE_ID, aiAvailable, showList, showSettings: false });
  const matchHeroes = heroRows(bundle);
  const deaths = deathRows(bundle);
  const timeCitations = [...noteText.matchAll(/\[[^\]]+\]\(#t=(\d{1,7})\)/g)].map(match => Number(match[1])).filter(Number.isFinite);

  async function selectEntry(entry: ReplayLibraryEntry) {
    setSelectedId(analyticsReplaySelection(entry).selectedId); setShowList(false); setCreatedNote(null); setNoteFallback(false); setError(null);
  }
  async function parseSelectedEntry(entry: ReplayLibraryEntry | null) {
    if (!entry || !["parse", "retry"].includes(analyticsReplayAction(entry.status) ?? "")) return;
    try { await libraryClient.parse(entry.match_id); void refresh(); }
    catch { setError(t(locale, "เริ่มแยกรีเพลย์ไม่ได้", "Could not parse the replay")); }
  }
  async function pickDem() {
    try { const path = await libraryClient.pick(); if (path) { await libraryClient.parse(path); void refresh(); } }
    catch { setError(t(locale, "เลือกหรือแยก .dem ไม่สำเร็จ", "Could not select or parse the .dem file")); }
  }
  async function togglePin(entry: ReplayLibraryEntry) {
    try { await libraryClient.pin(entry.match_id, !entry.pinned); void refresh(); }
    catch { setError(t(locale, "บันทึกสถานะปักหมุดไม่ได้", "Could not update pin")); }
  }
  async function loadOrphans() {
    setOrphansLoading(true); setOrphansError(false);
    try { setOrphans(orphanRows(await invoke<unknown>(ANALYTICS_ORPHANS_COMMAND))); }
    catch { setOrphansError(true); }
    finally { setOrphansLoading(false); }
  }
  async function toggleDataManagement() {
    const open = !showDataManagement;
    setShowDataManagement(open);
    setDataManagementError(null);
    if (!open) { setDeleteState("closed"); return; }
    setShowList(false);
    await loadOrphans();
  }
  async function deleteSelectedAnalysis() {
    const transition = analyticsDeleteTransition(deleteState, "confirm", selectedId);
    if (!transition.command) return;
    setDataManagementError(null);
    setDeleteState(transition.state);
    try {
      await invoke(transition.command.name, transition.command.args);
      setDeleteState(analyticsDeleteTransition("deleting", "complete", selectedId).state);
      setKnowledge(null); setNotes([]); setLoadedReport(null); setSelectedReportId(null);
      setDataManagementError(t(locale, "ลบข้อมูลวิเคราะห์ของแมตช์นี้แล้ว", "Deleted analysis data for this match"));
      await loadOrphans();
    } catch {
      setDeleteState("confirming");
      setDataManagementError(t(locale, "ลบข้อมูลวิเคราะห์ไม่ได้", "Could not delete analysis data"));
    }
  }
  async function deleteAiNote(noteId: string) {
    if (!selectedId) return;
    try {
      await invoke("analytics_note_delete", { matchId: selectedId, noteId });
      setNoteDeleteConfirmId(null); setOpenNote(SYSTEM_NOTE_ID); void refreshKnowledge(selectedId);
    } catch {
      setNoteDeleteConfirmId(null);
      setError(t(locale, "ลบโน้ตไม่สำเร็จ", "Could not delete the note"));
    }
  }
  async function ask() {
    if ((!question.trim() && !quoteChips.length) || streaming || !selectedId) return;
    const promptQuestion = selectionPrompt(question, quoteChips, locale);
    if (!aiAvailable) {
      if (prefs.ai_on && !prefs.consent) { setConsentOpen(true); return; }
      const localAnswer = localTemplate(bundle, promptQuestion, ownHero ?? selected?.own_hero ?? null, selected, locale);
      setAnswer(localAnswer);
      setTranscript(turns => [...turns, { role: "user", content: promptQuestion }, { role: "assistant", content: localAnswer }]);
      setUsage(null); setQuestion(""); setQuoteChips([]); setNoteFallback(false);
      return;
    }
    if (knowledge?.chat_history_over_budget) {
      setError(t(locale, "ประวัติแชตเกินงบแล้ว เริ่มบทสนทนาใหม่จากสรุปก่อนส่งคำถาม", "Chat history is over budget. Start a new conversation from its local recap before sending."));
      return;
    }
    setError(null); setAnswer(""); setUsage(null); setToolCalls([]); setCreatedNote(null); setNoteFallback(false); setStreaming(true);
    requestMode.current = "chat";
    try {
      const id = await invoke<string>("analytics_chat_submit", {
        matchId: selectedId, question: promptQuestion, selectedHero: hero, scrubberTimeS: currentTime,
      });
      activeRequestId.current = id; setRequestId(id); setQuestion(""); setQuoteChips([]);
      setToolCalls(pendingToolCalls.current.get(id) ?? []); pendingToolCalls.current.delete(id);
      setTranscript(turns => [...turns, { role: "user", content: promptQuestion }, { role: "assistant", content: "" }]);
      const pending = pendingEvents.current.get(id); pendingEvents.current.delete(id);
      if (pending?.deltas.length) {
        const delta = pending.deltas.join("");
        if (requestMode.current === "chat") {
          setAnswer(delta);
          setTranscript(turns => appendAssistantDelta(turns, delta));
        }
      }
      if (pending?.complete) finishRequest.current({ ...pending.complete, request_id: id });
    } catch (reason) {
      requestMode.current = null;
      setStreaming(false);
      setError(friendlyAnalyticsError(reason, locale, t(locale, "ส่งคำถามไม่สำเร็จ", "Could not send the question")));
    }
  }
  async function generateReport(regenerate: boolean) {
    if (!selectedId || !bundle || streaming || !aiAvailable) return;
    setError(null); setToolCalls([]); setKnowledgeTab("ai"); setStreaming(true); requestMode.current = "report";
    try {
      const result = await invoke<{ cached?: boolean; report?: ReportDetail | null; request_id?: string | null }>("analytics_report_generate", {
        matchId: selectedId, selectedHero: hero, scrubberTimeS: currentTime, regenerate,
      });
      if (result.cached && result.report) {
        requestMode.current = null; setStreaming(false);
        setLoadedReport(result.report); setSelectedReportId(result.report.id);
        void refreshKnowledge(selectedId);
        return;
      }
      const id = result.request_id;
      if (typeof id !== "string" || !id) throw new Error("Report request was not started");
      activeRequestId.current = id; setRequestId(id);
      setToolCalls(pendingToolCalls.current.get(id) ?? []); pendingToolCalls.current.delete(id);
      const pending = pendingEvents.current.get(id); pendingEvents.current.delete(id);
      if (pending?.complete) finishRequest.current({ ...pending.complete, request_id: id });
    } catch (reason) {
      requestMode.current = null; setStreaming(false);
      setError(friendlyAnalyticsError(reason, locale, t(locale, "สร้างบทวิเคราะห์ไม่สำเร็จ", "Could not generate the report")));
    }
  }
  async function startNewChatFromSummary() {
    if (!selectedId || streaming || !knowledge?.chat_history_over_budget || historyUnavailable || !transcript.length) return;
    const message = t(locale,
      "บันทึกสรุปบทสนทนา 24 ข้อความล่าสุดไว้เป็นโน้ตในเครื่อง แล้วเริ่มประวัติแชตใหม่หรือไม่? ประวัติเดิมจะถูกแทนที่หลังบันทึกโน้ตสำเร็จ",
      "Save a local recap of the latest 24 turns as a note and start a new chat? The old transcript will be replaced only after the note is saved.");
    if (!window.confirm(message)) return;
    try {
      const summary = buildChatRecap(transcript, 12_000);
      const noteId = await invoke<string>("analytics_chat_start_from_summary", { matchId: selectedId, summary });
      setQuestion(""); setAnswer(""); setUsage(null); setCreatedNote(null);
      setOpenNote(noteId); setKnowledgeTab("notes");
      const [turns] = await Promise.all([refreshTranscript(selectedId), refreshKnowledge(selectedId)]);
      setTranscript(turns);
    } catch (reason) {
      setError(t(locale, "เริ่มบทสนทนาใหม่ไม่ได้", "Could not start a new conversation"));
    }
  }
  async function saveAnswerAsNote() {
    if (!selectedId || !answer.trim() || !shouldOfferSaveAsNote({ answer, savedNoteId: createdNote, noteFallback })) return;
    try {
      const noteId = await invoke<string>("analytics_note_save_text", {
        matchId: selectedId,
        title: locale === "th" ? "คำตอบ AI" : "AI answer",
        markdown: answer,
      });
      setCreatedNote(noteId);
      setNoteFallback(false);
      void refreshKnowledge(selectedId);
    } catch {
      setError(locale === "th" ? "บันทึกโน้ตไม่ได้" : "Could not save the note");
    }
  }
  const identifyOwnHero = useCallback((heroId: string) => {
    if (!selectedId) return;
    void invoke("analytics_own_hero_set", { matchId: selectedId, hero: heroId }).then(() => { setOwnHero(heroId); void refresh(); })
      .catch(() => setError(t(locale, "บันทึกฮีโร่ของฉันไม่ได้", "Could not save your hero")));
  }, [selectedId, locale, refresh]);
  const onViewerReady = useCallback((root: ShadowRoot) => {
    viewerRoot.current = root;
    setPlayerSlot(root.getElementById("analytics-player-slot"));
    setPlayerPlaying(false);
    setPlayerSpeed(10);
    const time = Number(root.querySelector<HTMLInputElement>("#rv-scrub")?.value);
    if (Number.isFinite(time)) setCurrentTime(time);
  }, []);
  const onTimeChange = useCallback((seconds: number) => setCurrentTime(seconds), []);
  const seek = useCallback((seconds: number) => {
    const start = typeof bundle?.match.t0 === "number" ? bundle.match.t0 : 0;
    const end = typeof bundle?.match.t1 === "number" ? bundle.match.t1 : start;
    const safe = Math.max(start, Math.min(end, Number.isFinite(seconds) ? seconds : start));
    setCurrentTime(safe);
    const range = viewerRoot.current?.querySelector<HTMLInputElement>("#rv-scrub");
    if (range) { range.value = String(safe); range.dispatchEvent(new Event("input", { bubbles: true })); }
  }, [bundle]);
  const handledMatchKey = useRef<number | null>(null);
  useEffect(() => {
    if (!openMatchKey || handledMatchKey.current === openMatchKey.nonce || !library) return;
    const entry = library.matches.find(row => row.match_key === openMatchKey.key);
    if (!entry) return;
    handledMatchKey.current = openMatchKey.nonce;
    setSelectedId(entry.match_id); setShowList(false); setError(null);
  }, [openMatchKey, library]);
  useEffect(() => {
    if (!restoreContext) return;
    setSelectedId(restoreContext.matchId);
    if (restoreContext.documentId === "ai-report") {
      setKnowledgeTab("ai");
      setSelectedReportId(null);
      setLeftCollapsed(false);
    } else {
      setKnowledgeTab("notes");
      setOpenNote(restoreContext.documentId);
      setLeftCollapsed(false);
    }
    seek(restoreContext.timeS);
  }, [restoreContext, seek]);
  useEffect(() => {
    const play = viewerRoot.current?.getElementById("rv-play");
    if (!playerSlot || !play) return;
    const sync = () => setPlayerPlaying(/หยุด|pause/i.test(play.getAttribute("aria-label") ?? play.textContent ?? ""));
    sync();
    const observer = new MutationObserver(sync);
    observer.observe(play, { attributes: true, childList: true, characterData: true, subtree: true });
    return () => observer.disconnect();
  }, [playerSlot]);
  useEffect(() => {
    if (!active) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (document.querySelector('dialog[open], [aria-modal="true"]')) return;
      const action = analyticsShortcut(event);
      if (action) {
        event.preventDefault();
        if (action === "ask-selection" && !developerMode) return;
        if (action === "toggle-side") updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: !current.collapsed.side } }));
        if (action === "toggle-cards") updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, ...toggleCardColumns(current.collapsed) } }));
        if (action === "toggle-assistant") {
          if (rightCollapsed) {
            updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: false, summary: false, assistant: false } }));
            window.requestAnimationFrame(() => composerRef.current?.focus());
          } else setRightCollapsed(true);
        }
        if (action === "reset-layout") { resetPanePreferences(preferenceStorage(), paneSizeClass, pageSize.width); resetHandle("all"); }
        if (action === "ask-selection") void useSelectionAction("ask");
        if (action === "note-selection") void useSelectionAction("note");
        return;
      }
      if (!event.ctrlKey && !event.altKey && !event.metaKey && event.key === "F6") {
        event.preventDefault();
        const regions = [
          viewerRoot.current?.querySelector<HTMLElement>(".rv-team-column.team-radiant .rc[tabindex='0']"),
          playerRange.current,
          viewerRoot.current?.querySelector<HTMLElement>(".rv-team-column.team-dire .rc[tabindex='0']"),
          pageRef.current?.querySelector<HTMLElement>("#analytics-summary"),
          pageRef.current?.querySelector<HTMLElement>(".analytics-composer textarea:not(:disabled)"),
        ].filter((node): node is HTMLElement => !!node && node.getClientRects().length > 0);
        if (regions.length) {
          const activeElement = viewerRoot.current?.activeElement ?? document.activeElement;
          const activeIndex = regions.findIndex(node => node === activeElement || node.contains(activeElement));
          const from = activeIndex >= 0 ? activeIndex : focusRegion.current >= 0 ? focusRegion.current : event.shiftKey ? 0 : regions.length - 1;
          focusRegion.current = (from + (event.shiftKey ? -1 : 1) + regions.length) % regions.length;
          regions[focusRegion.current]!.focus();
        }
        return;
      }
      if (!event.ctrlKey && !event.altKey && !event.metaKey && event.key.toLowerCase() === "i") {
        const target = event.composedPath().find(item => item instanceof Element);
        if (!(target instanceof Element) || !target.closest("input,textarea,select,button")) {
          event.preventDefault();
          viewerRoot.current?.dispatchEvent(new CustomEvent("analytics-status-open-selected", { bubbles: true }));
        }
        return;
      }
      if (event.altKey || event.ctrlKey || event.metaKey) return;
      const path = event.composedPath();
      const origin = path.find((item): item is Element => item instanceof Element);
      const targetType = origin?.tagName === "INPUT"
        ? ((origin as HTMLInputElement).type === "range" ? "range" : "input")
        : origin?.tagName.toLowerCase();
      const playback = analyticsPlaybackKeyAction(event.key, targetType);
      if (!playback) return;
      event.preventDefault();
      if (playback.type === "toggle") viewerRoot.current?.getElementById("rv-play")?.click();
      else seek(currentTime + playback.deltaSeconds);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [active, currentTime, seek, rightCollapsed, pageSize.width, paneSizeClass, developerMode]);
  function onMarkdownAction(event: MouseEvent<HTMLDivElement>) {
    const target = (event.target as Element | null)?.closest<HTMLElement>("[data-seek-time],[data-seek-event],[data-doc-id],[data-md-copy-url],[data-md-copy-code]");
    if (!target) return;
    const action = actionFromMarkdownTarget(target); const root = viewerRoot.current;
    if (action?.kind === "time") seek(action.seconds);
    if (action?.kind === "event" && root && action.table === "deaths") root.querySelector<HTMLButtonElement>(`#rv-dlist [data-index="${action.index}"]`)?.click();
    if (action?.kind === "document") {
      const destination = analyticsDocumentDestination(action.id, notes.map(note => note.id));
      if (destination === "report") {
        setKnowledgeTab("ai");
        if (action.id === "ai-report") {
          if (!selectedReportId && knowledge?.reports?.[0]) setSelectedReportId(knowledge.reports[0].id);
        } else setSelectedReportId(action.id);
        setLeftCollapsed(false);
      }
      if (destination === "note") { setOpenNote(action.id); setKnowledgeTab("notes"); setLeftCollapsed(false); }
    }
    if (action?.kind === "copy_url") void navigator.clipboard?.writeText(action.url);
    if (action?.kind === "copy_code") {
      const code = target.closest(".an-md-code-block")?.querySelector("pre code")?.textContent;
      if (code !== undefined) void navigator.clipboard?.writeText(code);
    }
  }

  // Listen on the document: a drag that starts in the summary often ends outside it, and the old
  // container-level mouseup then never fired (owner's 88f4276 checklist c9, 2026-11-25).
  const captureSelectionRef = useRef<(container: HTMLElement, source: string) => void>(() => {});
  useEffect(() => {
    const onRelease = () => {
      const selection = window.getSelection();
      if (!selection || selection.isCollapsed || !selection.rangeCount) return;
      const node = selection.getRangeAt(0).commonAncestorContainer;
      const element = node instanceof Element ? node : node.parentElement;
      const container = element?.closest<HTMLElement>("[data-analytics-quotable]");
      if (container) captureSelectionRef.current(container, container.dataset.analyticsQuotable || "summary");
    };
    document.addEventListener("mouseup", onRelease);
    document.addEventListener("keyup", onRelease);
    return () => { document.removeEventListener("mouseup", onRelease); document.removeEventListener("keyup", onRelease); };
  }, []);
  function captureSelection(container: HTMLElement, source: string) {
    const selection = window.getSelection();
    if (!selection || selection.isCollapsed || !selection.rangeCount) return;
    const range = selection.getRangeAt(0);
    if (!container.contains(range.startContainer) || !container.contains(range.endContainer)) return;
    const startElement = range.startContainer instanceof Element ? range.startContainer : range.startContainer.parentElement;
    const timeMarker = startElement?.closest<HTMLElement>("[data-seek-time],[data-time],[data-time-s],a[href*=\"#t=\"]");
    const rawTime = timeMarker?.dataset.seekTime ?? timeMarker?.dataset.time ?? timeMarker?.dataset.timeS
      ?? timeMarker?.getAttribute("href")?.match(/#t=(\d{1,7})/)?.[1];
    const contextTime = rawTime !== undefined && rawTime !== null && Number.isFinite(Number(rawTime)) ? Number(rawTime) : null;
    const sourceLabel = source === "summary"
      ? t(locale, "สรุป › " + knowledgeTab, "Summary › " + knowledgeTab)
      : t(locale, "ผู้ช่วย", "Assistant");
    const quote = selectionQuote({ text: selection.toString(), source: sourceLabel, fallbackTime: contextTime });
    if (!quote.text) return;
    const rect = range.getBoundingClientRect();
    setSelectionAction({ quote, rect });
  }
  captureSelectionRef.current = captureSelection;

  async function useSelectionAction(action: "ask" | "seek" | "note") {
    const quote = selectionAction?.quote;
    if (!quote) return;
    if (action === "ask" && !developerMode) return;
    setSelectionAction(null);
    if (action === "seek") {
      if (quote.time !== null) seek(quote.time);
      return;
    }
    if (action === "ask") {
      setQuoteChips(current => [...current.slice(-2), quote]);
      updatePanePreferences(current => ({
        ...current,
        collapsed: { ...current.collapsed, side: false, summary: false, assistant: false },
      }));
      window.requestAnimationFrame(() => composerRef.current?.focus());
      return;
    }
    if (!selectedId) return;
    const timestamp = quote.time === null ? "" : ` (${formatAnalyticsTime(quote.time)})`;
    const markdown = `> ${quote.text}\n\n— ${quote.source}${timestamp}`;
    try {
      const noteId = await invoke<string>("analytics_note_save_text", {
        matchId: selectedId,
        title: t(locale, `ข้อความที่เลือก${timestamp}`, `Selected text${timestamp}`),
        markdown,
      });
      setCreatedNote(noteId);
      void refreshKnowledge(selectedId);
    } catch {
      setError(t(locale, "บันทึกข้อความที่เลือกไม่ได้", "Could not save the selected text"));
    }
  }

  const documentTitle = openNote === SYSTEM_NOTE_ID ? t(locale, "สรุปแมตช์.md", "Match summary.md") : notes.find(note => note.id === openNote)?.title ?? openNote;
  const aiVisibility = analyticsAiVisibility(developerMode, prefs.show_untested);
  const actualRightCollapsed = rightCollapsed;
  const layout = analyticsPaneLayout({
    width: pageSize.width || 1366,
    height: pageSize.height || 768,
    preferences: panePreferences,
  });
  const sideCollapsed = layout.collapsed.side;
  const topBarLayout = analyticsTopBarLayout(shellBarWidth || pageSize.width);
  const layoutStyle = {
    "--an-card-w": `${layout.cardWidth}px`,
    "--an-r-card-w": `${layout.cardWidths.radiant}px`,
    "--an-d-card-w": `${layout.cardWidths.dire}px`,
    "--analytics-side-track": `${sideCollapsed ? 40 : layout.sideWidth}px`,
    "--analytics-pane-split": `${Math.round(panePreferences.split * 100)}%`,
    "--analytics-summary-height": `${Math.max(40, Math.round((pageSize.height - 8) * panePreferences.split))}px`,
    "--analytics-map-side": `${layout.mapSide}px`,
    "--hc-scale": layout.scale,
  } as CSSProperties;
  const resetHandle = (target: "cards" | "side" | "split" | "all") => {
    const defaults = defaultPanePreferences(paneSizeClass, pageSize.width);
    updatePanePreferences(current => ({
      ...current,
      cardWidth: target === "cards" || target === "all" ? defaults.cardWidth : current.cardWidth,
      sideWidth: target === "side" || target === "all" ? defaults.sideWidth : current.sideWidth,
      split: target === "split" || target === "all" ? defaults.split : current.split,
      collapsed: target === "all" ? defaults.collapsed : {
        ...current.collapsed,
        ...(target === "cards" ? { radiant: false, dire: false } : {}),
        ...(target === "side" ? { side: defaults.collapsed.side } : {}),
        ...(target === "split" ? { summary: false, assistant: false } : {}),
      },
    }));
  };
  const beginPaneResize = (event: ReactPointerEvent<HTMLDivElement>, target: "card-left" | "card-right" | "side" | "split") => {
    event.preventDefault();
    try { event.currentTarget.setPointerCapture(event.pointerId); } catch { /* Window listeners remain available. */ }
    const startX = event.clientX;
    const startY = event.clientY;
    const start = panePreferencesRef.current;
    let pending = start;
    let finished = false;
    let pendingFrame = 0;
    let latestPointer: PointerEvent | null = null;
    let move = (_pointer: PointerEvent) => {};
    const compute = (pointer: PointerEvent) => {
      const dx = pointer.clientX - startX;
      const dy = pointer.clientY - startY;
      if (target === "side") {
        const width = start.sideWidth - dx;
        pending = width < 244
          ? { ...start, collapsed: { ...start.collapsed, side: true } }
          : { ...start, sideWidth: width, collapsed: { ...start.collapsed, side: false } };
      } else if (target === "split") {
        const split = start.split + dy / Math.max(1, pageSize.height - 8);
        const available = pageSize.height - 8;
        if (available * split < 72) pending = { ...start, collapsed: { ...start.collapsed, summary: true, assistant: false } };
        else if (available * (1 - split) < 72) pending = { ...start, collapsed: { ...start.collapsed, summary: false, assistant: true } };
        else pending = { ...start, split, collapsed: { ...start.collapsed, summary: false, assistant: false } };
      } else {
        const side = target === "card-left" ? "radiant" : "dire";
        const width = start.cardWidth + dx * (target === "card-left" ? 1 : -1);
        pending = width < 176
          ? { ...start, collapsed: { ...start.collapsed, [side]: true } }
          : { ...start, cardWidth: width, collapsed: { ...start.collapsed, [side]: false } };
      }
    };
    const finish = () => {
      if (finished) return;
      finished = true;
      if (pendingFrame) window.cancelAnimationFrame(pendingFrame);
      pendingFrame = 0;
      if (latestPointer) compute(latestPointer);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      commitPanePreferences(pending);
    };
    move = (pointer: PointerEvent) => {
      latestPointer = pointer;
      if (pendingFrame) return;
      pendingFrame = window.requestAnimationFrame(() => {
        pendingFrame = 0;
        if (latestPointer) compute(latestPointer);
        previewPanePreferences(pending);
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
    window.addEventListener("pointercancel", finish, { once: true });
  };
  const onPaneHandleKey = (event: ReactKeyboardEvent<HTMLDivElement>, target: "card-left" | "card-right" | "side" | "split") => {
    if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const minimum = event.key === "Home";
      if (target === "card-left" || target === "card-right") updatePanePreferences(current => ({
        ...current, cardWidth: minimum ? 232 : 420,
        collapsed: { ...current.collapsed, [target === "card-left" ? "radiant" : "dire"]: false },
      }));
      if (target === "side") updatePanePreferences(current => ({ ...current, sideWidth: minimum ? 300 : 640, collapsed: { ...current.collapsed, side: false } }));
      if (target === "split") updatePanePreferences(current => ({ ...current, split: minimum ? 0.25 : 0.75, collapsed: { ...current.collapsed, summary: false, assistant: false } }));
      return;
    }
    if (["Enter", " "].includes(event.key)) {
      event.preventDefault();
      if (target === "card-left" || target === "card-right") {
        const side = target === "card-left" ? "radiant" : "dire";
        updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, [side]: !current.collapsed[side] } }));
      }
      if (target === "side") updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: !current.collapsed.side } }));
      if (target === "split") setRightCollapsed(!rightCollapsed);
      return;
    }
    const directions = target === "split" ? { ArrowUp: -1, ArrowDown: 1 } : { ArrowLeft: -1, ArrowRight: 1 };
    const direction = directions[event.key as keyof typeof directions];
    if (direction === undefined) return;
    event.preventDefault();
    const amount = event.shiftKey ? 64 : 16;
    updatePanePreferences(current => {
      if (target === "card-left" || target === "card-right") {
        const side = target === "card-left" ? "radiant" : "dire";
        const width = current.cardWidth + direction * amount * (target === "card-left" ? 1 : -1);
        return { ...current, cardWidth: Math.max(232, Math.min(420, width)), collapsed: { ...current.collapsed, [side]: false } };
      }
      if (target === "side") {
        const width = current.sideWidth - direction * amount;
        return { ...current, sideWidth: Math.max(300, Math.min(640, width)), collapsed: { ...current.collapsed, side: false } };
      }
      const split = current.split + direction * amount / Math.max(1, pageSize.height - 8);
      return { ...current, split: Math.max(0.25, Math.min(0.75, split)), collapsed: { ...current.collapsed, summary: false, assistant: false } };
    });
  };
  const reportRows: ReportDetail[] = (knowledge?.reports ?? []).map(report => loadedReport?.id === report.id ? loadedReport : report as ReportDetail);
  if (loadedReport && !reportRows.some(report => report.id === loadedReport.id)) reportRows.unshift(loadedReport);
  const providerControls = <ProviderControls locale={locale} developerMode={developerMode} settings={prefs}
    onSettingsChange={setPrefs} onKeyStatusChange={setKeyStatus} compact consentOpen={consentOpen} onConsentOpenChange={setConsentOpen} />;
  const duration = selected?.duration_s == null ? UNKNOWN_VALUE : clock(selected.duration_s);
  const timelineStart = typeof bundle?.match.t0 === "number" ? bundle.match.t0 : 0;
  const timelineEnd = typeof bundle?.match.t1 === "number" ? bundle.match.t1 : null;
  const durationSeconds = bundle?.durationS ?? null;
  const timelineLength = timelineEnd !== null ? Math.max(1, timelineEnd - timelineStart) : 1;
  const timePercent = (seconds: number) => Math.max(0, Math.min(100, (seconds - timelineStart) / timelineLength * 100));
  const playbackLabel = `${formatAnalyticsTime(currentTime)} / ${formatAnalyticsTime(durationSeconds)}`;
  const heroName = matchHeroes.find(item => item.id === (selected?.own_hero ?? ownHero))?.name;
  const selectedHeroName = typeof heroName === "string" ? heroName
    : UNKNOWN_VALUE;
  const matchHeroId = selected?.own_hero ?? ownHero ?? null;
  const matchHeroIcon = matchHeroId ? BUNDLED.icons.get(`hero:${matchHeroId.replace(/^npc_dota_hero_/, "").toLowerCase()}`) : null;
  const result = selected?.local_history?.result === "win" ? t(locale, "ชนะ", "Win")
    : selected?.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE;
  const matchPickerLabel = [thaiDate(selected, locale), selectedHeroName, result, duration,
    selected ? statusText(selected.status, locale) : t(locale, "เลือกรีเพลย์", "Choose replay")].filter(Boolean).join(" · ");
  const player = playerSlot && bundle && timelineEnd !== null && timelineEnd > timelineStart ? createPortal(
    <>
      <style>{analyticsPlayerShadowCss}</style>
      <div className="analytics-player" aria-label={copy(locale).analyticsPlayer.label}>
        <div className="analytics-player-controls">
          <button type="button" className="analytics-player-play" aria-label={playerPlaying ? copy(locale).analyticsPlayer.pause : copy(locale).analyticsPlayer.play}
            title={playerPlaying ? copy(locale).analyticsPlayer.pause : copy(locale).analyticsPlayer.play}
            onClick={() => viewerRoot.current?.getElementById("rv-play")?.click()}>{playerPlaying ? "❚❚" : "▶"}</button>
          <label className="analytics-player-speed">
            <span className="analytics-player-visually-hidden">{copy(locale).analyticsPlayer.speed}</span>
            <select aria-label={copy(locale).analyticsPlayer.speed} value={playerSpeed} onChange={event => {
              const speed = Number(event.currentTarget.value);
              setPlayerSpeed(speed);
              viewerRoot.current?.getElementById(`sp${speed}`)?.click();
            }}>{[1, 5, 10, 30].map(speed => <option key={speed} value={speed}>{speed}×</option>)}</select>
          </label>
          <output className="analytics-player-clock" aria-live="off">{playbackLabel}</output>
          <button type="button" className="analytics-player-details" onClick={() => {
            if (!selectedId) return;
            if (playerPlaying) viewerRoot.current?.getElementById("rv-play")?.click();
            onOpenReplay?.(enterAnalyticsReplayDetails(bundle, {
              matchId: selectedId, timeS: currentTime,
              documentId: knowledgeTab === "ai" ? "ai-report" : openNote,
            }));
          }}>{copy(locale).analyticsPlayer.details} ›</button>
        </div>
        <div className="analytics-player-track" onPointerLeave={() => setHoverTime(null)}>
          <input ref={playerRange} type="range" min={timelineStart} max={timelineEnd} step="1" value={Math.max(timelineStart, Math.min(timelineEnd, currentTime))}
            aria-label={copy(locale).analyticsPlayer.time}
            style={{ "--played": `${timePercent(currentTime)}%` } as CSSProperties}
            onChange={event => seek(Number(event.currentTarget.value))}
            onPointerMove={event => {
              const rect = event.currentTarget.getBoundingClientRect();
              const percent = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
              setHoverTime({ seconds: timelineStart + percent * timelineLength, percent: percent * 100 });
            }} />
          <div className="analytics-player-ticks" aria-label={t(locale, "เหตุการณ์บนแถบเวลา", "Events on the timeline")}>
            {deaths.map((death, index) => {
              const seconds = Number(death.t);
              const team = analyticsDeathTeam(death, matchHeroes);
              if (!Number.isFinite(seconds) || seconds < timelineStart || seconds > timelineEnd) return null;
              return <button key={`d${index}`} type="button" className={`analytics-player-death analytics-player-team-${team ?? "unknown"}`}
                style={{ left: `${timePercent(seconds)}%` }} title={`† ${formatAnalyticsTime(seconds)}`}
                aria-label={`${copy(locale).analyticsPlayer.deaths}: ${formatAnalyticsTime(seconds)}`} onClick={() => seek(seconds)}>†</button>;
            })}
            {timeCitations.filter(seconds => seconds >= timelineStart && seconds <= timelineEnd).map((seconds, index) => <button key={`c${index}`}
              type="button" className="analytics-player-citation" style={{ left: `${timePercent(seconds)}%` }} title={`◆ ${formatAnalyticsTime(seconds)}`}
              aria-label={`${t(locale, "อ้างอิงในเอกสาร", "Document citation")}: ${formatAnalyticsTime(seconds)}`} onClick={() => seek(seconds)}>◆</button>)}
          </div>
          {hoverTime && <output className="analytics-player-hover-time" style={{ left: `${hoverTime.percent}%` }}>{formatAnalyticsTime(hoverTime.seconds)}</output>}
        </div>
        <div className="analytics-player-death-strip" aria-label={copy(locale).analyticsPlayer.deaths}>
          {deaths.flatMap((death, index) => {
            const seconds = Number(death.t);
            if (!Number.isFinite(seconds) || Math.abs(seconds - currentTime) > 60) return [];
            const team = analyticsDeathTeam(death, matchHeroes);
            return [<button key={index} type="button" className={`analytics-player-team-${team ?? "unknown"}`}
              title={`${copy(locale).analyticsPlayer.deaths}: ${formatAnalyticsTime(seconds)}`} onClick={() => seek(seconds)}>† {formatAnalyticsTime(seconds)}</button>];
          }).slice(0, 12)}
          {!deaths.some(death => Number.isFinite(Number(death.t)) && Math.abs(Number(death.t) - currentTime) <= 60)
            && <span>{copy(locale).analyticsPlayer.noDeaths}</span>}
        </div>
      </div>
    </>, playerSlot) : null;
  const shellToolbar = shellToolsTarget ? createPortal(
    <div className="analytics-topbar" data-toolbar-size={shellBarWidth >= 1181 ? "wide" : shellBarWidth >= 981 ? "medium" : shellBarWidth >= 760 ? "narrow" : "compact"}>
      <div className="analytics-replay-select">
          <button type="button" className="analytics-match-step" aria-label={t(locale, "รีเพลย์ก่อนหน้า", "Previous replay")} title={t(locale, "รีเพลย์ก่อนหน้า", "Previous replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index - 1 + rows.length) % rows.length]!);
        }}>‹</button>
        <button ref={matchPickerButton} type="button" className="analytics-match-picker" aria-label={matchPickerLabel} title={matchPickerLabel} aria-haspopup="menu" aria-expanded={matchPickerOpen} onClick={() => setMatchPickerOpen(open => !open)}>
          <span className="analytics-match-avatar" aria-hidden="true">{matchHeroIcon ? <img src={matchHeroIcon} alt="" /> : <span>?</span>}</span>
          <span className="analytics-match-date">{thaiDate(selected, locale)}</span>
          <span className="analytics-match-hero">{selectedHeroName}</span>
          <span className="analytics-match-result" data-result={selected?.local_history?.result ?? "unknown"}>{result}</span>
          <span className="analytics-match-duration">{duration}</span>
          <span className="analytics-match-status">{selected ? statusText(selected.status, locale) : t(locale, "เลือกรีเพลย์", "Choose replay")}</span>
        </button>
        <button type="button" className="analytics-match-step" aria-label={t(locale, "รีเพลย์ถัดไป", "Next replay")} title={t(locale, "รีเพลย์ถัดไป", "Next replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index + 1) % rows.length]!);
        }}>›</button>
        {matchPickerOpen && <div className="analytics-shell-menu analytics-match-menu" role="menu" aria-label={t(locale, "เลือกรีเพลย์", "Choose replay")} onKeyDown={moveMenuFocus}>
          {(library?.matches ?? []).map(entry => <button role="menuitem" key={entry.match_id} onClick={() => { void selectEntry(entry); setMatchPickerOpen(false); }}>
            {thaiDate(entry, locale)} · {displayHeroName(entry.own_hero)} · {entry.local_history?.result === "win" ? t(locale, "ชนะ", "Win") : entry.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE} · {statusText(entry.status, locale)}
          </button>)}
        </div>}
      </div>
      {selected && analyticsReplayAction(selected.status) && analyticsReplayAction(selected.status) !== "reparse" && <button type="button" className="analytics-parse-action" aria-label={t(locale, "แยกรีเพลย์", "Parse replay")} title={t(locale, "แยกรีเพลย์", "Parse replay")} onClick={() => void parseSelectedEntry(selected)}><span aria-hidden="true">↻</span><span className="analytics-toolbar-label">{t(locale, "แยกรีเพลย์", "Parse replay")}</span></button>}
      {bundle && topBarLayout.hero !== "more" && <label className="analytics-own-hero">
        <span>{topBarLayout.hero === "portraitLabel" ? t(locale, "ฮีโร่ของฉัน", "My hero") : t(locale, "ฮีโร่", "Hero")}</span>
        <select value={ownHeroChoice} aria-label={t(locale, "เลือกฮีโร่ของฉัน", "Choose my hero")} onChange={event => setOwnHeroChoice(event.currentTarget.value)}>
          <option value="">{t(locale, "เลือกฮีโร่", "Choose hero")}</option>
          {matchHeroes.filter(item => typeof item.id === "string").map(item => <option key={String(item.id)} value={String(item.id)}>{typeof item.name === "string" ? item.name : String(item.id).replace("npc_dota_hero_", "")}</option>)}
        </select>
        <button type="button" disabled={!ownHeroChoice} onClick={() => identifyOwnHero(ownHeroChoice)}>{t(locale, "ตั้งค่า", "Set")}</button>
      </label>}
      <span className="analytics-toolbar-status" role="status" title={selected ? statusText(selected.status, locale) : t(locale, "ยังไม่เลือกรีเพลย์", "No replay selected")}>
        <i aria-hidden="true" data-status={selected?.status.state ?? "empty"} />
        {topBarLayout.status === "dotLabel" && <span>{selected ? statusText(selected.status, locale) : t(locale, "พร้อม", "Ready")}</span>}
      </span>
      <button type="button" className="analytics-toolbar-action" aria-pressed={layout.collapsed.radiant && layout.collapsed.dire}
        aria-label={t(locale, "สลับการ์ดฮีโร่", "Toggle hero cards")} title={t(locale, "สลับการ์ดฮีโร่", "Toggle hero cards")} onClick={() => updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, ...toggleCardColumns(current.collapsed) } }))}>
        <span aria-hidden="true">▤</span><span className="analytics-toolbar-label">{t(locale, "การ์ด", "Cards")}</span>
      </button>
      <button type="button" className="analytics-toolbar-action" aria-pressed={sideCollapsed}
        aria-label={t(locale, "สลับแผงวิเคราะห์", "Toggle analysis pane")} title={t(locale, "สลับแผงวิเคราะห์", "Toggle analysis pane")} onClick={() => updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: !current.collapsed.side } }))}>
        <span aria-hidden="true">◫</span><span className="analytics-toolbar-label">{t(locale, "แผง", "Pane")}</span>
      </button>
      {topBarLayout.buttons.includes("cardSettings") && <button ref={element => { if (element) cardSettingsTrigger.current = element; }} type="button" className="analytics-toolbar-action" aria-expanded={cardSettingsOpen}
        aria-label={t(locale, "การตั้งค่าการ์ดฮีโร่", "Hero card settings")} title={t(locale, "การ์ดฮีโร่", "Hero card")}
        onClick={event => { cardSettingsTrigger.current = event.currentTarget; setCardSettingsOpen(open => !open); }}><span aria-hidden="true">☷</span><span className="analytics-toolbar-label">{t(locale, "การ์ดฮีโร่", "Hero card")}</span></button>}
      {cardSettingsOpen && <div className="analytics-shell-menu analytics-card-menu">
        <HeroCardDisplayControls value={heroCardDisplay} locale={locale} onChange={changeHeroCardDisplay} compact />
        <button type="button" onClick={() => { setCardSettingsOpen(false); onOpenHeroCardSettings?.(); }}>{t(locale, "เปิดการตั้งค่าการ์ดฮีโร่", "Open hero card settings")}</button>
      </div>}
      {topBarLayout.buttons.includes("layout") && <div className="analytics-toolbar-menu-wrap">
        <button ref={layoutMenuButton} type="button" className="analytics-toolbar-action" aria-haspopup="menu" aria-expanded={layoutMenuOpen} aria-label={t(locale, "จัดวางแผง", "Pane layout")} title={t(locale, "จัดวางแผง", "Pane layout")} onClick={() => setLayoutMenuOpen(open => !open)}>
          <span aria-hidden="true">⛶</span><span className="analytics-toolbar-label">{t(locale, "จัดวาง", "Layout")}</span>
        </button>
        {layoutMenuOpen && <div className="analytics-shell-menu" role="menu" onKeyDown={moveMenuFocus}>
          <button role="menuitem" onClick={() => setLeftCollapsed(!leftCollapsed)}>{leftCollapsed ? t(locale, "เปิดสรุป", "Open summary") : t(locale, "ยุบสรุป", "Collapse summary")}</button>
          <button role="menuitem" onClick={() => setRightCollapsed(!rightCollapsed)}>{rightCollapsed ? t(locale, "เปิดผู้ช่วย", "Open assistant") : t(locale, "ยุบผู้ช่วย", "Collapse assistant")}</button>
          <button role="menuitem" onClick={() => { resetPanePreferences(preferenceStorage(), paneSizeClass, pageSize.width); resetHandle("all"); }}>{t(locale, "คืนค่าเลย์เอาต์", "Reset layout")}</button>
        </div>}
      </div>}
      <div className="analytics-toolbar-menu-wrap">
        <button ref={moreMenuButton} type="button" className="analytics-toolbar-action analytics-more" aria-haspopup="menu" aria-expanded={moreMenuOpen} aria-label={t(locale, "เพิ่มเติม", "More")} title={t(locale, "เพิ่มเติม", "More")} onClick={event => { cardSettingsTrigger.current = event.currentTarget; setMoreMenuOpen(open => !open); }}>
          <span aria-hidden="true">⋯</span><span className="analytics-toolbar-label">{t(locale, "เพิ่มเติม", "More")}</span>
        </button>
        {moreMenuOpen && <div className="analytics-shell-menu" role="menu" onKeyDown={moveMenuFocus}>
          {topBarLayout.overflow.includes("cardSettings") && <button role="menuitem" onClick={() => { cardSettingsTrigger.current = moreMenuButton.current; setCardSettingsOpen(true); setMoreMenuOpen(false); }}>{t(locale, "การ์ดฮีโร่", "Hero card settings")}</button>}
          {topBarLayout.overflow.includes("layout") && <>
            <button role="menuitem" onClick={() => setLeftCollapsed(!leftCollapsed)}>{leftCollapsed ? t(locale, "เปิดสรุป", "Open summary") : t(locale, "ยุบสรุป", "Collapse summary")}</button>
            <button role="menuitem" onClick={() => setRightCollapsed(!rightCollapsed)}>{rightCollapsed ? t(locale, "เปิดผู้ช่วย", "Open assistant") : t(locale, "ยุบผู้ช่วย", "Collapse assistant")}</button>
            <button role="menuitem" onClick={() => { resetPanePreferences(preferenceStorage(), paneSizeClass, pageSize.width); resetHandle("all"); }}>{t(locale, "คืนค่าเลย์เอาต์", "Reset layout")}</button>
          </>}
          {topBarLayout.hero === "more" && bundle && <label className="analytics-menu-own-hero">{t(locale, "ฮีโร่ของฉัน", "My hero")}
            <select value={ownHeroChoice} aria-label={t(locale, "เลือกฮีโร่ของฉัน", "Choose my hero")} onChange={event => setOwnHeroChoice(event.currentTarget.value)}>
              <option value="">{t(locale, "เลือกฮีโร่", "Choose hero")}</option>
              {matchHeroes.filter(item => typeof item.id === "string").map(item => <option key={String(item.id)} value={String(item.id)}>{typeof item.name === "string" ? item.name : String(item.id).replace("npc_dota_hero_", "")}</option>)}
            </select><button type="button" disabled={!ownHeroChoice} onClick={() => identifyOwnHero(ownHeroChoice)}>{t(locale, "ตั้งค่า", "Set")}</button>
          </label>}
          <button role="menuitem" onClick={() => void toggleDataManagement()}>{t(locale, "ข้อมูลวิเคราะห์", "Analysis data")}</button>
          <button role="menuitem" onClick={() => { setMoreMenuOpen(false); setShowDataManagement(false); setShowList(true); }}>{t(locale, "คลังรีเพลย์", "Replay library")}</button>
          {consentVersionNotice && <button role="menuitem" className="analytics-consent-version-notice" onClick={() => setConsentOpen(true)}>{t(locale, "อ่านข้อความยินยอมอีกครั้ง", "Review updated consent")}</button>}
          <p>{t(locale, "ปุ่มลัด: Ctrl+\\ แผง · Ctrl+Shift+\\ การ์ด · Ctrl+J ผู้ช่วย · Ctrl+Alt+0 คืนค่า · F6 เปลี่ยนพื้นที่ · I บัฟ", "Shortcuts: Ctrl+\\ pane · Ctrl+Shift+\\ cards · Ctrl+J assistant · Ctrl+Alt+0 reset · F6 region · I effects")}</p>
        </div>}
      </div>
      {aiVisibility.showUnavailableNotice && <span className="analytics-assistant-unavailable-dot" role="status" title={t(locale, "ผู้ช่วย AI ยังไม่เปิดให้ใช้ในรุ่นนี้", "AI assistant is unavailable in this build")}>●</span>}
    </div>, shellToolsTarget) : null;
  return <div ref={pageRef} className="analytics-page" style={layoutStyle} data-state={state} data-layout={layout.density}
    data-size-class={layout.sizeClass} data-side-collapsed={sideCollapsed} data-summary-collapsed={leftCollapsed}
    data-assistant-collapsed={actualRightCollapsed} data-assistant-unavailable={String(!developerMode)}
    data-cards-collapsed={layout.collapsed.radiant && layout.collapsed.dire}
    data-selection-action={selectionAction ? "open" : "closed"}>
    {shellToolbar}
    {layout.tooSmall ? <div className="analytics-enlarge" role="status">
      <strong>{t(locale, "ขยายหน้าต่างเพื่อดู Analytics", "Enlarge the window to view Analytics")}</strong>
      <span>{t(locale, "แผงจะยุบเป็นแถบเมื่อพื้นที่ไม่พอ และการ์ดจะคงขนาดอย่างน้อย 85%", "Panels collapse to rails when space is limited; cards stay at least 85% scale")}</span>
    </div> : null}
    <div className="analytics-columns" aria-hidden={layout.tooSmall}>
      {sideCollapsed && <aside className="analytics-side-rail" aria-label={t(locale, "แผงวิเคราะห์", "Analysis pane")}>
        <button type="button" aria-label={t(locale, "เปิดแผงสรุป", "Open summary pane")} onClick={() => updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: false, summary: false } }))}>▤</button>
        <button type="button" aria-label={t(locale, "เปิดแผงผู้ช่วย", "Open assistant pane")} onClick={() => updatePanePreferences(current => ({ ...current, collapsed: { ...current.collapsed, side: false, assistant: false } }))}>✦</button>
      </aside>}
      {!leftCollapsed ? <aside id="analytics-summary" tabIndex={-1} className="analytics-column analytics-docs"><header><button onClick={() => setLeftCollapsed(true)} aria-label={t(locale, "ยุบส่วนสรุป", "Collapse summary")}>⌄</button><strong>{t(locale, "สรุป", "Summary")}</strong></header>
        {createdNote && <button type="button" className="analytics-created-note" onClick={() => { setOpenNote(createdNote); setKnowledgeTab("notes"); setCreatedNote(null); }}>{t(locale, "จดโน้ตแล้ว · ดูโน้ต", "Note saved · view note")}</button>}
        <section className="analytics-knowledge-slot" aria-label={t(locale, "แท็บความรู้และโน้ตแมตช์", "Match knowledge and notes tabs")} data-knowledge-tabs-slot>
          <KnowledgeTabs activeTab={knowledgeTab} onSelectTab={setKnowledgeTab} />
          <div className="analytics-knowledge-panel" data-analytics-quotable="summary">
            {knowledgeTab === "summary" && <MatchSummaryTab digest={knowledge?.digest ?? null} loading={!knowledge && !!selectedId} onSeek={seek} mapCalibration={summaryMapCalibration} />}
            {knowledgeTab === "compare" && <HeroComparisonTab digest={knowledge?.digest ?? null} selectedHeroId={hero ?? ownHero ?? knowledge?.own_hero_id ?? null}
              opponentHeroId={compareHero} onSelectHero={setHero} onSelectOpponentHero={setCompareHero} />}
            {knowledgeTab === "ai" && <AiReportTab reports={reportRows} selectedReportId={selectedReportId ?? reportRows[0]?.id ?? null}
              onSelectReport={id => { setSelectedReportId(id); setLoadedReport(null); }}
              onGenerateReport={() => void generateReport(false)} onRegenerateReport={() => void generateReport(true)}
              generatingReport={streaming && requestMode.current === "report"} reportDisabled={!bundle || !aiAvailable}
              costEstimateText={knowledge?.report_cost_reason ?? t(locale, "ตรวจสอบค่าใช้จ่ายไม่ได้จนกว่าจะยืนยันราคาผู้ให้บริการ", "Provider pricing is unverified; cost cannot be estimated before sending.")}
              renderMarkdown={content => <div className="analytics-markdown" onClick={onMarkdownAction} dangerouslySetInnerHTML={{ __html: renderMarkdownSafe(content, { resolveChartReference: chartResolver }) }} />} />}
            {knowledgeTab === "notes" && <div className="analytics-note-tab">
              <div className="analytics-doc-tabs" role="tablist" aria-label={t(locale, "เอกสาร", "Documents")}>
                <button role="tab" aria-selected={openNote === SYSTEM_NOTE_ID} onClick={() => setOpenNote(SYSTEM_NOTE_ID)}>{t(locale, "สรุปแมตช์.md", "Match summary.md")}</button>
                {notes.map(note => <button role="tab" key={note.id} aria-selected={openNote === note.id} onClick={() => setOpenNote(note.id)}>{note.ai && <b>AI</b>}{note.title}</button>)}
              </div>
              <div className="analytics-doc-toolbar"><strong title={documentTitle}>{documentTitle}</strong><button onClick={() => void navigator.clipboard?.writeText(noteText)}>{t(locale, "คัดลอก", "Copy")}</button>
                <button onClick={() => setShowRawSource(value => !value)}>{showRawSource ? t(locale, "ดูตัวอย่าง", "Preview") : t(locale, "ดูต้นฉบับ .md", "View source .md")}</button>
                <button onClick={() => selectedId && void invoke("analytics_note_open_folder", { matchId: selectedId })}>{t(locale, "เปิดโฟลเดอร์", "Open folder")}</button>
                {openNote !== SYSTEM_NOTE_ID && notes.some(note => note.id === openNote && note.ai) && (noteDeleteConfirmId === openNote
                  ? <span className="analytics-note-delete-confirm" role="group" aria-label={t(locale, "ยืนยันการลบโน้ต", "Confirm note deletion")}>
                    <span>{t(locale, "ยืนยันลบโน้ตนี้หรือไม่?", "Delete this note?")}</span>
                    <button type="button" onClick={() => void deleteAiNote(openNote)}>{t(locale, "ยืนยันการลบ", "Confirm delete")}</button>
                    <button type="button" onClick={() => setNoteDeleteConfirmId(null)}>{t(locale, "ยกเลิก", "Cancel")}</button>
                  </span>
                  : <button type="button" onClick={() => setNoteDeleteConfirmId(openNote)}>{t(locale, "ลบโน้ต", "Delete note")}</button>)}
              </div>
              {showRawSource ? <pre className="analytics-markdown analytics-source">{noteText}</pre> : <div className="analytics-markdown" onClick={onMarkdownAction} dangerouslySetInnerHTML={{ __html: markdown }} />}
            </div>}
          </div>
        </section>
      </aside> : <button className="analytics-expand analytics-summary-rail" aria-label={t(locale, "เปิดสรุป", "Open summary")} onClick={() => setLeftCollapsed(false)}>▤</button>}
      <main className="analytics-column analytics-centre">
        {!bundle && <div className="analytics-empty" role="status">{state === "empty" ? t(locale, "ยังไม่มีรีเพลย์ในคลัง", "No replays in the library") : selected?.status.state === "parsing" || selected?.status.state === "queued" ? t(locale, statusText(selected.status, "th"), statusText(selected.status, "en")) : t(locale, "เลือกรีเพลย์ที่พร้อมดู", "Select a replay that is ready to view")}</div>}
        {selected?.status.state === "failed" && <p className="analytics-error" role="status">{t(locale, "แยกรีเพลย์ไม่สำเร็จ ตรวจสอบไฟล์แล้วลองอีกครั้งได้", "Replay parsing failed. Check the file and try again.")}</p>}
        {bundle && <div className="analytics-viewer-frame">
          <MatchViewerSurface locale={locale} bundle={bundle} onHeroSelect={setHero} statLabelMode="icon" heroCardDisplay={heroCardDisplay}
            selectedHeroId={hero ?? ownHero ?? selected?.own_hero ?? knowledge?.own_hero_id ?? null} density={layout.density}
            cardsCollapsed={{ radiant: layout.collapsed.radiant, dire: layout.collapsed.dire }}
            onTimeChange={onTimeChange} onReady={onViewerReady} initialTime={0} disableKeyboard />
          <div className="analytics-resize-handle analytics-handle-cards-left" role="separator" tabIndex={0} aria-orientation="vertical"
            aria-valuenow={layout.cardWidth} aria-valuemin={232} aria-valuemax={420} aria-controls="rv-cards-r"
            aria-label={t(locale, "ปรับความกว้างการ์ด Radiant", "Resize Radiant cards")}
            onPointerDown={event => beginPaneResize(event, "card-left")} onDoubleClick={() => resetHandle("cards")} onKeyDown={event => onPaneHandleKey(event, "card-left")} />
          <div className="analytics-resize-handle analytics-handle-cards-right" role="separator" tabIndex={0} aria-orientation="vertical"
            aria-valuenow={layout.cardWidth} aria-valuemin={232} aria-valuemax={420} aria-controls="rv-cards-d"
            aria-label={t(locale, "ปรับความกว้างการ์ด Dire", "Resize Dire cards")}
            onPointerDown={event => beginPaneResize(event, "card-right")} onDoubleClick={() => resetHandle("cards")} onKeyDown={event => onPaneHandleKey(event, "card-right")} />
        </div>}
        {player}
      </main>
      <div className="analytics-resize-handle analytics-handle-side" role="separator" tabIndex={0} aria-orientation="vertical"
        aria-valuenow={layout.sideWidth} aria-valuemin={300} aria-valuemax={640} aria-controls="analytics-side-pane"
        aria-label={t(locale, "ปรับความกว้างแผงวิเคราะห์", "Resize analysis pane")}
        onPointerDown={event => beginPaneResize(event, "side")} onDoubleClick={() => resetHandle("side")} onKeyDown={event => onPaneHandleKey(event, "side")} />
      <aside id="analytics-side-pane" className="analytics-side-pane">
        {!leftCollapsed && !actualRightCollapsed && <div className="analytics-resize-handle analytics-handle-split" role="separator" tabIndex={0} aria-orientation="horizontal"
          aria-valuenow={Math.round(panePreferences.split * 100)} aria-valuemin={25} aria-valuemax={75} aria-controls="analytics-summary analytics-assistant"
          aria-label={t(locale, "ปรับสัดส่วนสรุปและผู้ช่วย", "Resize summary and assistant")}
          onPointerDown={event => beginPaneResize(event, "split")} onDoubleClick={() => resetHandle("split")} onKeyDown={event => onPaneHandleKey(event, "split")} />}
        {!actualRightCollapsed && developerMode && <aside id="analytics-assistant" className="analytics-column analytics-chat"><header className="analytics-chat-status"><strong>{t(locale, "ผู้ช่วย", "Assistant")}</strong><span>{prefs.provider} · {prefs.model || UNKNOWN_VALUE} · {streaming ? t(locale, "กำลังตอบ…", "Responding…") : aiAvailable ? t(locale, "พร้อม", "Ready") : t(locale, "ยังไม่เปิด", "Unavailable")}</span><button onClick={() => setRightCollapsed(true)} aria-label={t(locale, "ยุบส่วนผู้ช่วย", "Collapse assistant")}>⌄</button></header>
        {providerControls}
        <div className="analytics-chat-feed" aria-live="polite" onClick={onMarkdownAction} data-analytics-quotable="assistant">
          {state === "noai" && <div className="analytics-noai"><strong>{t(locale, "เปิดใช้ผู้ให้บริการ AI ในการตั้งค่า", "Enable an AI provider in Settings")}</strong><p>{t(locale, "ถามด้วยแม่แบบในเครื่องได้ คำถามเหล่านี้ไม่ส่งข้อมูลออกจากเครื่อง", "Use local templates; these questions do not send data off-device.")}</p>
            <div className="analytics-template-questions">{ANALYTICS_TEMPLATE_QUESTIONS.map(item => <button key={item.id} onClick={() => {
              const prompt = locale === "th" ? item.th : item.en;
              const localAnswer = localTemplate(bundle, prompt, ownHero ?? selected?.own_hero ?? null, selected, locale);
              setAnswer(localAnswer); setTranscript(turns => [...turns, { role: "user", content: prompt }, { role: "assistant", content: localAnswer }]);
            }}>{locale === "th" ? item.th : item.en}</button>)}</div>
          </div>}
          {historyUnavailable && <p className="analytics-error" role="status">{t(locale, "ระบบหลักยังอ่านประวัติแชตจากไฟล์ได้ไม่สำเร็จ", "The host could not load the saved chat transcript.")}</p>}
          {knowledge && ((knowledge.chat_history_estimated_tokens ?? 0) / Math.max(1, knowledge.chat_history_token_budget ?? 12_000) >= 0.7 || knowledge.chat_history_over_budget === true) && <section className="analytics-history-budget" data-over-budget={knowledge.chat_history_over_budget === true}>
            <span>{t(locale, "งบประวัติแชต", "Chat history budget")}: {knowledge.chat_history_estimated_tokens ?? 0}/{knowledge.chat_history_token_budget ?? 12_000} tokens</span>
            {knowledge.chat_history_over_budget && <><p>{t(locale, "ประวัติถูกเก็บไว้ในเครื่อง เมื่อเกินงบ ระบบจะไม่ส่งคำถามจนกว่าคุณจะบันทึกสรุปและเริ่มบทสนทนาใหม่", "History stays on this device. Once over budget, questions are blocked until you save a local recap and start a new conversation.")}</p>
              <button type="button" disabled={streaming || historyUnavailable || !transcript.length} onClick={() => void startNewChatFromSummary()}>{t(locale, "เริ่มบทสนทนาใหม่จากสรุปในเครื่อง", "Start a new conversation from a local recap")}</button></>}
          </section>}
          {toolCalls.map((line, index) => <p className="analytics-tool-line" key={`${selectedId ?? "none"}-tool-${index}`}>{line}</p>)}
          {transcript.map((turn, index) => <article className="analytics-chat-turn" data-role={turn.role} key={`${selectedId ?? "none"}-${index}`}>
            <div dangerouslySetInnerHTML={{ __html: renderMarkdownStreaming(turn.content, { resolveChartReference: chartResolver }) }} />
            {turn.role === "assistant" && usage && index === transcript.length - 1 && <small>{t(locale, "อินพุต", "Input")} {usage.input ?? UNKNOWN_VALUE} · cache {usage.cache_read ?? UNKNOWN_VALUE} · {t(locale, "เอาต์พุต", "Output")} {usage.output ?? UNKNOWN_VALUE}</small>}
          </article>)}
          {shouldOfferSaveAsNote({ answer, savedNoteId: createdNote, noteFallback }) && <button type="button" onClick={() => void saveAnswerAsNote()}>{AI_PROVIDER_COPY[locale].noteFallback}</button>}
          {createdNote && <button className="analytics-created-note" onClick={() => { setOpenNote(createdNote); setKnowledgeTab("notes"); setLeftCollapsed(false); setCreatedNote(null); }}>{t(locale, "สร้างแล้ว · เปิดในแผงซ้าย", "Created · open in left panel")}: {createdNote}</button>}
          {error && <p className="analytics-error" role="alert">{error}</p>}
        </div>

        <form className="analytics-composer" onSubmit={event => { event.preventDefault(); void ask(); }}>
          {!prefs.consent && <button type="button" onClick={() => setConsentOpen(true)}>{AI_PROVIDER_COPY[locale].consent}</button>}
          {quoteChips.map((quote, index) => <button type="button" className="analytics-quote-chip" key={`${quote.source}-${quote.time}-${index}`}
            title={t(locale, "นำข้อความที่เลือกออก", "Remove selected text")} onClick={() => setQuoteChips(chips => chips.filter((_, item) => item !== index))}>
            {quote.time === null ? t(locale, "ข้อความที่เลือก", "Selected text") : formatAnalyticsTime(quote.time)} · {quote.source} · {quote.text}
          </button>)}
          <textarea ref={composerRef} value={question} onChange={event => setQuestion(event.currentTarget.value)} onKeyDown={event => {
            if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void ask(); }
          }} placeholder={t(locale, "ถามเกี่ยวกับแมตช์นี้…", "Ask about this match…")} disabled={streaming || !bundle} />
          <div><span>{AI_PROVIDER_COPY[locale].sendDisclosure}</span>
            {streaming ? <button type="button" onClick={() => { if (requestId) void invoke("analytics_chat_cancel", { requestId }); requestMode.current = null; activeRequestId.current = null; setRequestId(null); setStreaming(false); }}>{t(locale, "ยกเลิก", "Cancel")}</button> : <button type="submit" disabled={(!question.trim() && !quoteChips.length) || !bundle || (aiAvailable && knowledge?.chat_history_over_budget === true)}>{t(locale, "ส่ง", "Send")}</button>}
          </div>
        </form>
      </aside>}
      {!actualRightCollapsed && !developerMode && <aside id="analytics-assistant" className="analytics-column analytics-chat is-unavailable">
        <header className="analytics-chat-status"><strong>{t(locale, "ผู้ช่วย", "Assistant")}</strong><span>{t(locale, "ยังไม่เปิดให้ใช้ในรุ่นนี้", "Unavailable in this build")}</span></header>
        <div className="analytics-assistant-unavailable" role="status">{t(locale, "ผู้ช่วย AI ยังไม่เปิดให้ใช้ในรุ่นนี้", "AI assistant is unavailable in this build")}</div>
        <form className="analytics-composer" onSubmit={event => event.preventDefault()}>
          {quoteChips.map((quote, index) => <button type="button" className="analytics-quote-chip" key={`${quote.source}-${quote.time}-${index}`} title={t(locale, "นำข้อความที่เลือกออก", "Remove selected text")} onClick={() => setQuoteChips(chips => chips.filter((_, item) => item !== index))}>
            {quote.time === null ? t(locale, "ข้อความที่เลือก", "Selected text") : formatAnalyticsTime(quote.time)} · {quote.source} · {quote.text}
          </button>)}
          <textarea value={question} disabled aria-label={t(locale, "ข้อความคำถาม", "Question text")} placeholder={t(locale, "ข้อความที่เลือกจะปรากฏที่นี่", "Selected text will appear here")} />
          <button type="button" disabled>{t(locale, "ส่ง", "Send")}</button>
        </form>
      </aside>}
      </aside>
      {actualRightCollapsed && <button className="analytics-expand analytics-assistant-rail" aria-label={t(locale, "เปิดผู้ช่วย", "Open assistant")} onClick={() => setRightCollapsed(false)}>✦</button>}
    </div>
    {selectionAction && <div className="analytics-selection-actions" role="toolbar" aria-label={t(locale, "การทำงานกับข้อความที่เลือก", "Actions for selected text")}
      style={{ left: Math.max(8, selectionAction.rect.left), top: Math.max(8, selectionAction.rect.top >= 56 ? selectionAction.rect.top - 48 : selectionAction.rect.bottom + 8) }}
      onMouseDown={event => event.preventDefault()}>
      {developerMode && <button type="button" onClick={() => void useSelectionAction("ask")}>{t(locale, "ถาม AI เรื่องนี้", "Ask AI about this")}</button>}
      {selectionAction.quote.time !== null && <button type="button" onClick={() => void useSelectionAction("seek")}>
        {t(locale, "ไปที่", "Go to")} {formatAnalyticsTime(selectionAction.quote.time)}
      </button>}
      <button type="button" onClick={() => void useSelectionAction("note")}>{t(locale, "จดโน้ต", "Save note")}</button>
      <button type="button" aria-label={t(locale, "ปิด", "Dismiss")} onClick={() => setSelectionAction(null)}>×</button>
    </div>}
    {showDataManagement && <div className="analytics-data-overlay" role="dialog" aria-modal="true" aria-labelledby="analytics-data-title"><section className="analytics-data-management">
      <header><h2 id="analytics-data-title">{t(locale, "ข้อมูลวิเคราะห์", "Analysis data")}</h2><button type="button" onClick={() => void toggleDataManagement()}>×</button></header>
      <p>{selected ? t(locale, "จัดการข้อมูลวิเคราะห์ที่บันทึกไว้สำหรับแมตช์ที่เลือก", "Manage saved analysis data for the selected match") : t(locale, "เลือกรายการแมตช์เพื่อจัดการข้อมูลวิเคราะห์", "Select a match to manage its analysis data")}</p>
      {dataManagementError && <p role="status" className={deleteState === "confirming" ? "analytics-error" : "analytics-data-message"}>{dataManagementError}</p>}
      <button type="button" disabled={!selectedId || deleteState === "deleting"} onClick={() => setDeleteState(analyticsDeleteTransition(deleteState, "request", selectedId).state)}>{t(locale, "ลบข้อมูลวิเคราะห์ของแมตช์นี้", "Delete analysis data for this match")}</button>
      {deleteState === "confirming" && <div className="analytics-delete-confirm" role="group" aria-label={t(locale, "ยืนยันการลบ", "Confirm deletion")}>
        <p>{t(locale, "ยืนยันลบข้อมูลวิเคราะห์ของแมตช์นี้หรือไม่?", "Delete this match's analysis data?")}</p>
        <button type="button" onClick={() => void deleteSelectedAnalysis()}>{t(locale, "ยืนยันการลบ", "Confirm delete")}</button>
        <button type="button" onClick={() => setDeleteState(analyticsDeleteTransition(deleteState, "cancel", selectedId).state)}>{t(locale, "ยกเลิก", "Cancel")}</button>
      </div>}
      <section aria-label={t(locale, "รายการข้อมูลที่ยังไม่มีแมตช์", "Orphaned analysis data")}>
        <h3>{t(locale, "รายการข้อมูลที่ยังไม่มีแมตช์", "Orphaned analysis data")}</h3>
        <button type="button" disabled={orphansLoading} onClick={() => void loadOrphans()}>{t(locale, "โหลดรายการอีกครั้ง", "Refresh list")}</button>
        {orphansLoading && <p role="status">{t(locale, "กำลังอ่านรายการ…", "Loading list…")}</p>}
        {orphansError && <p className="analytics-error" role="alert">{t(locale, "อ่านรายการข้อมูลไม่ได้", "Could not read the orphan list")}</p>}
        {!orphansLoading && !orphansError && (orphans.length === 0 ? <p>{t(locale, "ไม่พบรายการข้อมูลที่ยังไม่มีแมตช์", "No orphaned analysis data")}</p> : <ul>{orphans.map((item, index) => {
          const bytes = item.bytes ?? item.size_bytes ?? item.storage_bytes;
          const time = item.created_at_unix_ms ?? item.modified_at_unix_ms;
          const facts = [typeof time === "number" && Number.isFinite(time) ? new Intl.DateTimeFormat(locale === "th" ? "th-TH" : "en-GB", { dateStyle: "medium", timeStyle: "short" }).format(new Date(time)) : null,
            typeof bytes === "number" && Number.isFinite(bytes) ? `${(bytes / 1024).toFixed(0)} KB` : null].filter(Boolean);
          return <li key={index}>{t(locale, "รายการข้อมูลที่ยังไม่มีแมตช์", "Orphaned analysis data")}{facts.length ? ` · ${facts.join(" · ")}` : ` · ${UNKNOWN_VALUE}`}</li>;
        })}</ul>)}
      </section>
    </section></div>}
    {showList && <div className="analytics-list-overlay" role="dialog" aria-modal="true" aria-label={t(locale, "คลังรีเพลย์", "Replay library")}><section>
      <header><strong>{t(locale, "คลังรีเพลย์", "Replay library")}</strong><button onClick={() => setShowList(false)}>×</button></header>
      <div className="analytics-library-filters" role="group" aria-label={t(locale, "ตัวกรองรีเพลย์", "Replay filters")}>
        <button aria-pressed={filter === "all"} onClick={() => setFilter("all")}>{t(locale, "ทั้งหมด", "All")}</button>
        <button aria-pressed={filter === "pinned"} onClick={() => setFilter("pinned")}>{t(locale, "ปักหมุด", "Pinned")}</button>
        <button aria-pressed={filter === "notAnalysed"} onClick={() => setFilter("notAnalysed")}>{t(locale, "ยังไม่แยก", "Not parsed")}</button>
      </div>
      <p className="analytics-storage">{library?.matches.filter(entry => entry.status.state === "ready" && !entry.pinned).length ?? 0}/{prefs.keep_newest} · {((library?.matches.reduce((sum, entry) => sum + entry.storage_bytes, 0) ?? 0) / 1024 ** 2).toFixed(0)} MB / {(prefs.max_bytes / 1024 ** 3).toFixed(1)} GB</p>
      <button type="button" onClick={() => void pickDem()}>{t(locale, "เพิ่มไฟล์ .dem…", "Add .dem file…")}</button>
      {rows.map(entry => <article key={entry.match_id}><button onClick={() => void selectEntry(entry)}>{statusText(entry.status, locale)} · {thaiDate(entry, locale)} · {entry.local_history?.result === "win" ? t(locale, "ชนะ", "Win") : entry.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE}</button>
        <button onClick={() => void togglePin(entry)}>{entry.pinned ? t(locale, "เลิกปักหมุด", "Unpin") : t(locale, "เก็บไว้", "Keep")}</button>
        {analyticsReplayAction(entry.status) === "reparse"
          ? <button disabled={entry.source_exists !== true}
              title={entry.source_exists === true
                ? t(locale, `แยกรีเพลย์นี้ใหม่ด้วย engine ปัจจุบัน (ตอนนี้ schema ${entry.bundle_schema ?? "ไม่ทราบ"}) ข้อมูลเดิมอยู่จนกว่าจะแยกเสร็จ`, `Re-parse with the current engine (now schema ${entry.bundle_schema ?? "unknown"}); the old data stays until it finishes`)
                : t(locale, "ไม่พบไฟล์ .dem ต้นฉบับแล้ว", "The source .dem file is gone")}
              onClick={() => void libraryClient.reparse(entry.match_id).then(() => refresh(), () => setError(t(locale, "แยกใหม่ไม่สำเร็จ", "Re-parse failed")))}>
              {t(locale, "แยกใหม่", "Re-parse")}{entry.bundle_schema ? ` · ${entry.bundle_schema}` : ""}</button>
          : ["parse", "retry"].includes(analyticsReplayAction(entry.status) ?? "") && <button onClick={() => void parseSelectedEntry(entry)}>{entry.status.state === "failed" ? t(locale, "ลองแยกอีกครั้ง", "Try parsing again") : t(locale, "แยกรีเพลย์นี้", "Parse this replay")}</button>}
        <button onClick={() => void invoke("replay_library_open_folder", { matchId: entry.match_id })}>{t(locale, "เปิดโฟลเดอร์", "Open folder")}</button>
        <button onClick={() => { if (window.confirm(t(locale, "นำรีเพลย์ออกจากคลังหรือไม่? โน้ตจะยังอยู่", "Remove replay from library? Notes will be kept"))) void libraryClient.remove(entry.match_id); }}>{t(locale, "นำออกจากคลัง", "Remove from library")}</button>
      </article>)}
    </section></div>}
  </div>;
}

function emptyEntry(): ReplayLibraryEntry {
  return { match_key: null, match_id: "", source: "manual_pick", engine_version: null, bundle_hash: null,
    parse_time_unix_ms: null, status: { state: "available" }, bundle_bytes: 0, storage_bytes: 0, created_at_unix_ms: 0,
    match_date_utc: null, own_hero: null, duration_s: null, patch: null };
}
