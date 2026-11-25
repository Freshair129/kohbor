import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { AI_PROVIDER_COPY, copy, type Locale } from "../copy";
import { readDeveloperMode } from "../shell/developerMode";
import type { ReplayLibraryEntry, ReplayLibraryList, ReplayStatus } from "../replay/replayLibraryClient";
import { MatchViewerSurface, analyticsPlayerShadowCss } from "../replay/MatchViewPanel";
import { admitLibraryBundle, type MatchBundle } from "../replay/matchBundle";
import { ANALYTICS_TEMPLATE_QUESTIONS, answerAnalyticsTemplate, clock } from "../replay/coach";
import { actionFromMarkdownTarget, renderMarkdownSafe, renderMarkdownStreaming } from "./analyticsMarkdown";
import { createAnalyticsChartResolver } from "./analyticsChart";
import {
  analyticsAiVisibility, analyticsBundleSelectionKey, analyticsConsentRePrompt, analyticsDeleteTransition, analyticsDocumentDestination, analyticsLayout,
  analyticsReplayAction, analyticsReplaySelection, ANALYTICS_ORPHANS_COMMAND, buildChatRecap,
  analyticsDeathTeam, analyticsPlaybackKeyAction, analyticsViewState, enterAnalyticsReplayDetails,
  formatAnalyticsTime, replayRowFacts, UNKNOWN_VALUE,
  type AnalyticsReplayDetailsRoute, type AnalyticsReturnContext,
  type AnalyticsDeleteState,
  type AnalyticsChatTurn as ChatTurn,
} from "./analyticsModel";
import { ProviderControls, type ProviderPreferences } from "./ProviderControls";
import {
  AiReportTab, HeroComparisonTab, KnowledgeTabs, MatchSummaryTab, MatchupStrip,
  type KnowledgeDigest, type SavedReport,
} from "./knowledge";
import { providerErrorKind, shouldOfferSaveAsNote } from "./provider-model.mjs";
import "./analytics.css";

type Note = { id: string; title: string; ai: boolean };
type KeyStatus = { provider: string; set: boolean; last4: string | null; requires_key?: boolean };
type Usage = { input: number | null; cache_read: number | null; cache_creation: number | null; output: number | null };
type ReportDetail = SavedReport & { stale_reasons?: string[] };
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
  };
  const label = labels[status.state]; return locale === "th" ? label[0] : label[1];
}

export function AnalyticsPage({ locale, onOpenReplay, restoreContext, openMatchKey = null, active = true }: {
  locale: Locale;
  onOpenReplay?: (route: AnalyticsReplayDetailsRoute) => void;
  restoreContext?: AnalyticsReturnContext | null;
  /** Open the library match whose install-keyed hash equals `key` (post-game panel). `nonce` repeats a request. */
  openMatchKey?: { key: string; nonce: number } | null;
  active?: boolean;
}) {
  const [library, setLibrary] = useState<ReplayLibraryList | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<MatchBundle | null>(null);
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
  const [leftCollapsed, setLeftCollapsed] = useState(true);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [readingMode, setReadingMode] = useState(false);
  const [pageSize, setPageSize] = useState({ width: 0, height: 0 });
  const pageRef = useRef<HTMLDivElement>(null);
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
    if (!question.trim() || streaming || !selectedId) return;
    const userQuestion = question.trim();
    if (!aiAvailable) {
      if (prefs.ai_on && !prefs.consent) { setConsentOpen(true); return; }
      const localAnswer = localTemplate(bundle, userQuestion, ownHero ?? selected?.own_hero ?? null, selected, locale);
      setAnswer(localAnswer);
      setTranscript(turns => [...turns, { role: "user", content: userQuestion }, { role: "assistant", content: localAnswer }]);
      setUsage(null); setQuestion(""); setNoteFallback(false);
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
        matchId: selectedId, question: userQuestion, selectedHero: hero, scrubberTimeS: currentTime,
      });
      activeRequestId.current = id; setRequestId(id); setQuestion("");
      setToolCalls(pendingToolCalls.current.get(id) ?? []); pendingToolCalls.current.delete(id);
      setTranscript(turns => [...turns, { role: "user", content: userQuestion }, { role: "assistant", content: "" }]);
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
      if (event.altKey || event.ctrlKey || event.metaKey || document.querySelector('dialog[open], [aria-modal="true"]')) return;
      const path = event.composedPath();
      const origin = path.find((item): item is Element => item instanceof Element);
      const targetType = origin?.tagName === "INPUT"
        ? ((origin as HTMLInputElement).type === "range" ? "range" : "input")
        : origin?.tagName.toLowerCase();
      const action = analyticsPlaybackKeyAction(event.key, targetType);
      if (!action) return;
      event.preventDefault();
      if (action.type === "toggle") viewerRoot.current?.getElementById("rv-play")?.click();
      else seek(currentTime + action.deltaSeconds);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [active, currentTime, seek]);
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

  const documentTitle = openNote === SYSTEM_NOTE_ID ? t(locale, "สรุปแมตช์.md", "Match summary.md") : notes.find(note => note.id === openNote)?.title ?? openNote;
  const developerMode = readDeveloperMode(window.localStorage);
  const aiVisibility = analyticsAiVisibility(developerMode, prefs.show_untested);
  const actualRightCollapsed = aiVisibility.chatCollapsed || rightCollapsed;
  const layout = analyticsLayout(pageSize.width, pageSize.height, leftCollapsed, actualRightCollapsed, readingMode);
  const layoutStyle = {
    "--analytics-docs-track": `${leftCollapsed ? 28 : layout.mode === "docked" ? layout.docs : 28}px`,
    "--analytics-chat-track": `${actualRightCollapsed ? 28 : layout.right}px`,
    "--analytics-drawer-width": `${layout.drawerWidth}px`,
    "--analytics-center-min": `${layout.minCenter}px`,
    "--hc-scale": layout.heroCardScale,
  } as CSSProperties;
  const lastLayoutMode = useRef(layout.mode);
  useEffect(() => {
    if (pageSize.width <= 0 || lastLayoutMode.current === layout.mode) return;
    lastLayoutMode.current = layout.mode;
    setLeftCollapsed(readingMode ? false : layout.mode !== "docked");
  }, [layout.mode, pageSize.width, readingMode]);
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
  const result = selected?.local_history?.result === "win" ? t(locale, "ชนะ", "Win")
    : selected?.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE;
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
  return <div ref={pageRef} className="analytics-page" style={layoutStyle} data-state={state} data-layout={layout.mode}
    data-left-collapsed={leftCollapsed} data-right-collapsed={actualRightCollapsed} data-reading-mode={readingMode}>
    <div className="analytics-topbar">
      <div className="analytics-replay-select">
        <button type="button" aria-label={t(locale, "รีเพลย์ก่อนหน้า", "Previous replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index - 1 + rows.length) % rows.length]!);
        }}>‹</button>
        <select aria-label={t(locale, "เลือกรีเพลย์", "Choose replay")} value={selectedId ?? ""} onChange={event => {
          const entry = library?.matches.find(item => item.match_id === event.currentTarget.value); if (entry) void selectEntry(entry);
        }}>
          <option value="">{loading ? t(locale, "กำลังโหลด…", "Loading…") : t(locale, "เลือกรีเพลย์", "Choose replay")}</option>
          {(library?.matches ?? []).map(entry => <option key={entry.match_id} value={entry.match_id}>{thaiDate(entry, locale)} · {displayHeroName(entry.own_hero)} · {entry.local_history?.result === "win" ? t(locale, "ชนะ", "Win") : entry.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE} · {statusText(entry.status, locale)}</option>)}
        </select>
        <button type="button" aria-label={t(locale, "รีเพลย์ถัดไป", "Next replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index + 1) % rows.length]!);
        }}>›</button>
      </div>
      <div className="analytics-row-facts">
        <span>{selected ? statusText(selected.status, locale) : UNKNOWN_VALUE}</span><span>{thaiDate(selected, locale)}</span>
        <span>{selectedHeroName}</span><span>{result}</span><span>{duration}</span>
      </div>
      <div className="analytics-top-actions">
        {selected && analyticsReplayAction(selected.status) && analyticsReplayAction(selected.status) !== "reparse" && <button type="button" onClick={() => void parseSelectedEntry(selected)}>{t(locale, "แยกรีเพลย์นี้", "Parse this replay")}</button>}
        {bundle && <label className="analytics-own-hero">{t(locale, "ฮีโร่ของฉัน", "My hero")}
          <select value={ownHeroChoice} aria-label={t(locale, "เลือกฮีโร่ของฉัน", "Choose my hero")} onChange={event => setOwnHeroChoice(event.currentTarget.value)}>
            <option value="">{t(locale, "เลือกฮีโร่", "Choose a hero")}</option>
            {matchHeroes.filter(item => typeof item.id === "string").map(item => <option key={String(item.id)} value={String(item.id)}>{typeof item.name === "string" ? item.name : String(item.id).replace("npc_dota_hero_", "")}</option>)}
          </select><button type="button" disabled={!ownHeroChoice} onClick={() => identifyOwnHero(ownHeroChoice)}>{t(locale, "นี่คือฉัน", "This is me")}</button>
        </label>}
        <button type="button" aria-pressed={readingMode} onClick={() => { const next = !readingMode; setReadingMode(next); if (next) setLeftCollapsed(false); }}>{t(locale, "โหมดอ่าน", "Reading mode")}</button>
        <button type="button" onClick={() => void toggleDataManagement()}>{t(locale, "ข้อมูลวิเคราะห์", "Analysis data")}</button>
        {consentVersionNotice && <button type="button" className="analytics-consent-version-notice" onClick={() => setConsentOpen(true)}>{t(locale, "ข้อความยินยอมเปลี่ยน · อ่านอีกครั้ง", "Consent text changed · read again")}</button>}
        <button type="button" onClick={() => { setShowDataManagement(false); setShowList(true); }}>{t(locale, "คลังรีเพลย์", "Replay library")}</button>
        {aiVisibility.showUnavailableNotice && <span className="analytics-ai-unavailable" role="status">ผู้ช่วย AI ยังไม่เปิดให้ใช้ในรุ่นนี้</span>}
      </div>
    </div>
    {layout.mode === "enlarge-window" ? <div className="analytics-enlarge" role="status">
      <strong>{t(locale, "ขยายหน้าต่างเพื่อดู Analytics", "Enlarge the window to view Analytics")}</strong>
      <span>{t(locale, `ต้องการพื้นที่กว้างอย่างน้อย ${Math.ceil(28 + layout.minCenter + layout.right + 20)} px และสูง 760 px`, `This view needs at least ${Math.ceil(28 + layout.minCenter + layout.right + 20)} px width and 760 px height`)}</span>
    </div> : null}
    <div className="analytics-columns" aria-hidden={layout.mode === "enlarge-window"}>
      {!leftCollapsed ? <aside className="analytics-column analytics-docs"><header><button onClick={() => setLeftCollapsed(true)} aria-label={t(locale, "ยุบแผงซ้าย", "Collapse left panel")}>‹</button><strong>{t(locale, "วิเคราะห์", "Analysis")}</strong></header>
        <section className="analytics-knowledge-slot" aria-label={t(locale, "แท็บความรู้และโน้ตแมตช์", "Match knowledge and notes tabs")} data-knowledge-tabs-slot>
          <KnowledgeTabs activeTab={knowledgeTab} onSelectTab={setKnowledgeTab} />
          <div className="analytics-knowledge-panel">
            {knowledgeTab === "summary" && <MatchSummaryTab digest={knowledge?.digest ?? null} loading={!knowledge && !!selectedId} onSeek={seek} />}
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
      </aside> : <button className="analytics-expand" onClick={() => setLeftCollapsed(false)}>›</button>}
      <main className="analytics-column analytics-centre">
        {!bundle && <div className="analytics-empty" role="status">{state === "empty" ? t(locale, "ยังไม่มีรีเพลย์ในคลัง", "No replays in the library") : selected?.status.state === "parsing" || selected?.status.state === "queued" ? t(locale, statusText(selected.status, "th"), statusText(selected.status, "en")) : t(locale, "เลือกรีเพลย์ที่พร้อมดู", "Select a replay that is ready to view")}</div>}
        {selected?.status.state === "failed" && <p className="analytics-error" role="status">{t(locale, "แยกรีเพลย์ไม่สำเร็จ ตรวจสอบไฟล์แล้วลองอีกครั้งได้", "Replay parsing failed. Check the file and try again.")}</p>}
        {bundle && <MatchupStrip digest={knowledge?.digest ?? null} selectedHeroId={hero ?? ownHero ?? knowledge?.own_hero_id ?? null} onSelectHero={setHero} />}
        {bundle && <MatchViewerSurface locale={locale} bundle={bundle} onHeroSelect={setHero} statLabelMode="text"
          onTimeChange={onTimeChange} onReady={onViewerReady} initialTime={0} disableKeyboard />}
        {player}
      </main>
      {!actualRightCollapsed && developerMode ? <aside className="analytics-column analytics-chat"><header className="analytics-chat-status"><strong>{t(locale, "ผู้ช่วย", "Assistant")}</strong><span>{prefs.provider} · {prefs.model || UNKNOWN_VALUE} · {streaming ? t(locale, "กำลังตอบ…", "Responding…") : aiAvailable ? t(locale, "พร้อม", "Ready") : t(locale, "ยังไม่เปิด", "Unavailable")}</span><button onClick={() => setRightCollapsed(true)} aria-label={t(locale, "ยุบแผงขวา", "Collapse right panel")}>›</button></header>
        {providerControls}
        <div className="analytics-chat-feed" aria-live="polite" onClick={onMarkdownAction}>
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
          <textarea value={question} onChange={event => setQuestion(event.currentTarget.value)} onKeyDown={event => {
            if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void ask(); }
          }} placeholder={t(locale, "ถามเกี่ยวกับแมตช์นี้…", "Ask about this match…")} disabled={streaming || !bundle} />
          <div><span>{AI_PROVIDER_COPY[locale].sendDisclosure}</span>
            {streaming ? <button type="button" onClick={() => { if (requestId) void invoke("analytics_chat_cancel", { requestId }); requestMode.current = null; activeRequestId.current = null; setRequestId(null); setStreaming(false); }}>{t(locale, "ยกเลิก", "Cancel")}</button> : <button type="submit" disabled={!question.trim() || !bundle || (aiAvailable && knowledge?.chat_history_over_budget === true)}>{t(locale, "ส่ง", "Send")}</button>}
          </div>
        </form>
      </aside> : developerMode ? <button className="analytics-expand" onClick={() => setRightCollapsed(false)}>‹</button> : null}
      {(actualRightCollapsed || !developerMode) && <div className="analytics-provider-anchor" hidden>{providerControls}</div>}
    </div>
    {showDataManagement && <div className="analytics-data-overlay" role="dialog" aria-modal="true" aria-labelledby="analytics-data-title"><section className="analytics-data-management">
      <header><h2 id="analytics-data-title">{t(locale, "ข้อมูลวิเคราะห์", "Analysis data")}</h2><button type="button" onClick={() => void toggleDataManagement()}>×</button></header>
      <p>{selected ? t(locale, "จัดการข้อมูลวิเคราะห์ที่บันทึกไว้สำหรับแมตช์ที่เลือก", "Manage saved analysis data for the selected match") : t(locale, "เลือกรายการแมตช์เพื่อจัดการข้อมูลวิเคราะห์", "Select a match to manage its analysis data")}</p>
      {dataManagementError && <p role="status" className={deleteState === "confirming" ? "analytics-error" : "analytics-data-message"}>{dataManagementError}</p>}
      <button type="button" disabled={!selectedId || deleteState === "deleting"} onClick={() => setDeleteState(analyticsDeleteTransition(deleteState, "request", selectedId).state)}>{t(locale, "ลบข้อมูลวิเคราะห์ของแมตช์นี้", "Delete analysis data for this match")}</button>
      {deleteState === "confirming" && <div className="analytics-delete-confirm" role="group" aria-label={t(locale, "ยืนยันการลบ", "Confirm deletion")}>
        <p>{t(locale, "ยืนยันลบข้อมูลวิเคราะห์ของแมตช์นี้หรือไม่?", "Delete this match's analysis data?")}</p>
        <button type="button" disabled={deleteState === "deleting"} onClick={() => void deleteSelectedAnalysis()}>{t(locale, "ยืนยันการลบ", "Confirm delete")}</button>
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
  return { match_id: "", source: "manual_pick", source_path: "", dem_sha256: null, engine_version: null, engine_sha256: null,
    parse_time_unix_ms: null, status: { state: "available" }, bundle_bytes: 0, storage_bytes: 0, created_at_unix_ms: 0,
    match_date_utc: null, own_hero: null, duration_s: null, patch: null };
}
