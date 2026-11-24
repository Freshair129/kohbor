import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { AI_PROVIDER_COPY, copy, type Locale } from "../copy";
import type { ReplayLibraryEntry, ReplayLibraryList, ReplayStatus } from "../replay/replayLibraryClient";
import { MatchViewerSurface, analyticsPlayerShadowCss } from "../replay/MatchViewPanel";
import { admitLibraryBundle, type MatchBundle } from "../replay/matchBundle";
import { ANALYTICS_TEMPLATE_QUESTIONS, answerAnalyticsTemplate, clock } from "../replay/coach";
import { actionFromMarkdownTarget, renderMarkdownSafe } from "./analyticsMarkdown";
import {
  analyticsDeathTeam, analyticsPlaybackKeyAction, analyticsViewState, enterAnalyticsReplayDetails,
  formatAnalyticsTime, replayRowFacts, UNKNOWN_VALUE,
  type AnalyticsReplayDetailsRoute, type AnalyticsReturnContext,
} from "./analyticsModel";
import { ProviderControls, type ProviderPreferences } from "./ProviderControls";
import { providerErrorKind, shouldOfferSaveAsNote } from "./provider-model.mjs";
import "./analytics.css";

type Note = { id: string; title: string; ai: boolean };
type KeyStatus = { provider: string; set: boolean; last4: string | null; requires_key?: boolean };
type Usage = { input: number | null; cache_read: number | null; cache_creation: number | null; output: number | null };

const t = (locale: Locale, thai: string, english: string) => locale === "th" ? thai : english;
const SYSTEM_NOTE_ID = "สรุปแมตช์.md";
const libraryClient = {
  list: () => invoke<ReplayLibraryList>("replay_library_list"),
  open: (matchId: string) => invoke<unknown>("replay_library_open", { matchId }),
  parse: (idOrPath: string) => invoke("replay_library_parse", { idOrPath }),
  pin: (matchId: string, pinned: boolean) => invoke("replay_library_pin", { matchId, pinned }),
  remove: (matchId: string) => invoke("replay_library_remove", { matchId }),
  pick: () => invoke<string | null>("replay_library_pick_file"),
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
function localTemplate(bundle: MatchBundle | null, question: string, ownHero: string | null, selected: ReplayLibraryEntry | null, locale: Locale): string {
  if (!bundle) return t(locale, "— ไม่ทราบ", "— unknown");
  const heroes = heroRows(bundle).flatMap(hero => typeof hero.id === "string" && typeof hero.name === "string" && typeof hero.team === "number"
    ? [{ id: hero.id, name: hero.name, team: hero.team }] : []);
  const deaths = deathRows(bundle).flatMap(death => typeof death.t === "number" && typeof death.hero === "string" && typeof death.killer === "string"
    ? [{ t: death.t, hero: death.hero, killer: death.killer }] : []);
  return answerAnalyticsTemplate({ question, locale, durationS: bundle.durationS, heroes, deaths, ownHero,
    localResult: selected?.local_history?.result ?? null });
}

function heroKillCounts(bundle: MatchBundle | null): { radiant: number | null; dire: number | null } {
  if (!bundle || !Array.isArray(bundle.match.deaths) || !Array.isArray(bundle.match.heroes)) return { radiant: null, dire: null };
  const teams = new Map((bundle.match.heroes as Record<string, unknown>[]).flatMap(hero => typeof hero.id === "string" && typeof hero.team === "number" ? [[hero.id, hero.team] as const] : []));
  const deaths = bundle.match.deaths as Record<string, unknown>[];
  if (deaths.length && !deaths.some(death => typeof death.killer === "string" && teams.has(death.killer))) return { radiant: null, dire: null };
  return { radiant: deaths.filter(death => typeof death.killer === "string" && teams.get(death.killer) === 2).length,
    dire: deaths.filter(death => typeof death.killer === "string" && teams.get(death.killer) === 3).length };
}

function statusText(status: ReplayStatus, locale: Locale) {
  const labels: Record<ReplayStatus["state"], [string, string]> = {
    available: ["ยังไม่วิเคราะห์", "Not analysed"], queued: ["เข้าคิวแล้ว", "Queued"], parsing: ["กำลังวิเคราะห์", "Analysing"],
    ready: ["พร้อม", "Ready"], needs_pack: ["ขาดข้อมูลแพ็ก", "Pack unavailable"], failed: ["วิเคราะห์ไม่สำเร็จ", "Analysis failed"],
  };
  const label = labels[status.state]; return locale === "th" ? label[0] : label[1];
}

export function AnalyticsPage({ locale, onOpenReplay, restoreContext, active = true }: {
  locale: Locale;
  onOpenReplay?: (route: AnalyticsReplayDetailsRoute) => void;
  restoreContext?: AnalyticsReturnContext | null;
  active?: boolean;
}) {
  const [library, setLibrary] = useState<ReplayLibraryList | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<MatchBundle | null>(null);
  const [notes, setNotes] = useState<Note[]>([]);
  const [openNote, setOpenNote] = useState(SYSTEM_NOTE_ID);
  const [showRawSource, setShowRawSource] = useState(false);
  const [noteText, setNoteText] = useState("");
  const [filter, setFilter] = useState<"all" | "pinned" | "notAnalysed">("all");
  const [showList, setShowList] = useState(false);
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [hero, setHero] = useState<string | null>(null);
  const [ownHero, setOwnHero] = useState<string | null>(null);
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ provider: "anthropic", set: false, last4: null, requires_key: true });
  const [prefs, setPrefs] = useState<ProviderPreferences>({ ai_on: false, provider: "anthropic", consent: false, model: "claude-opus-5-5", provider_endpoints: {}, provider_consents: {}, verified_providers: {}, tools_modes: {}, show_untested: false, keep_newest: 20, max_bytes: 2 * 1024 ** 3, auto_analyse: true });
  const [consentOpen, setConsentOpen] = useState(false);
  const [streaming, setStreaming] = useState(false);
  const [question, setQuestion] = useState("");
  const [answer, setAnswer] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [usage, setUsage] = useState<Usage | null>(null);
  const [createdNote, setCreatedNote] = useState<string | null>(null);
  const [noteFallback, setNoteFallback] = useState(false);
  const [loading, setLoading] = useState(true);
  const [requestId, setRequestId] = useState<string | null>(null);
  const activeRequestId = useRef<string | null>(null);
  const pendingEvents = useRef(new Map<string, { deltas: string[]; complete?: { error?: string; usage?: Usage; note_id?: string; note_fallback?: boolean } }>());
  const finishRequest = useRef<(payload: { request_id: string; error?: string; usage?: Usage; note_id?: string; note_fallback?: boolean }) => void>(() => {});
  const [currentTime, setCurrentTime] = useState(0);
  const [playerSlot, setPlayerSlot] = useState<HTMLElement | null>(null);
  const [playerSpeed, setPlayerSpeed] = useState(10);
  const [playerPlaying, setPlayerPlaying] = useState(false);
  const [hoverTime, setHoverTime] = useState<{ seconds: number; percent: number } | null>(null);
  const [includeContext, setIncludeContext] = useState({ match: true, time: true, document: true, myHero: true, hero: true });
  const viewerRoot = useRef<ShadowRoot | null>(null);
  const playerRange = useRef<HTMLInputElement>(null);
  const markdown = useMemo(() => renderMarkdownSafe(noteText), [noteText]);

  const refresh = useCallback(async () => {
    try {
      const next = await libraryClient.list(); setLibrary(next);
      if (!selectedId && next.matches.length) setSelectedId(next.matches.find(row => row.status.state === "ready")?.match_id ?? next.matches[0]!.match_id);
    } catch { setError(t(locale, "อ่านคลังรีเพลย์ไม่ได้", "Could not read the replay library")); }
    finally { setLoading(false); }
  }, [locale, selectedId]);
  const refreshAi = useCallback(async () => {
    try {
      const [key, settings, items] = await Promise.all([
        invoke<KeyStatus>("analytics_key_status"), invoke<ProviderPreferences>("analytics_settings_get"),
        invoke<Note[]>("analytics_notes_list", { matchId: selectedId }),
      ]);
      setKeyStatus(key); setPrefs(settings); setNotes(items);
    } catch { /* The local page stays usable when AI is unavailable. */ }
  }, [selectedId]);

  useEffect(() => {
    let disposed = false; let unlisten: (() => void) | undefined;
    void refresh();
    void listen("replay-library://state-changed", () => { if (!disposed) void refresh(); }).then(stop => { if (disposed) stop(); else unlisten = stop; }).catch(() => {});
    return () => { disposed = true; unlisten?.(); };
  }, [refresh]);
  useEffect(() => { void refreshAi(); }, [refreshAi]);
  useEffect(() => {
    let alive = true; setBundle(null); setNoteText(""); setOpenNote(SYSTEM_NOTE_ID);
    setShowRawSource(false); setCurrentTime(0);
    if (!selectedId || library?.matches.find(entry => entry.match_id === selectedId)?.status.state !== "ready") return;
    void libraryClient.open(selectedId).then(raw => {
      const result = admitLibraryBundle(raw);
      if (alive && result.ok) setBundle(result.bundle);
      else if (alive) setError(t(locale, "เปิดชุดข้อมูลรีเพลย์ไม่ได้", "Could not open the replay bundle"));
    }).catch(() => { if (alive) setError(t(locale, "รีเพลย์นี้อาจถูกนำออกจากคลังแล้ว", "This replay may have been removed from the library")); });
    return () => { alive = false; };
  }, [selectedId, library, locale]);
  useEffect(() => {
    setShowRawSource(false);
    if (!selectedId) { setNoteText(systemNote(bundle, locale)); return; }
    void invoke<string>("analytics_note_read", { matchId: selectedId, noteId: openNote }).then(setNoteText)
      .catch(() => setNoteText(openNote === "สรุปแมตช์.md" ? systemNote(bundle, locale) : t(locale, "เปิดโน้ตไม่ได้ รีเพลย์อาจถูกนำออกจากคลัง แต่โน้ตยังอยู่", "Could not open the note. The replay may be removed, but notes are retained.")));
  }, [openNote, bundle, locale, selectedId]);
  finishRequest.current = payload => {
    if (payload.request_id !== activeRequestId.current) return;
    setStreaming(false); setUsage(payload.usage ?? null); setRequestId(null); activeRequestId.current = null;
    setNoteFallback(payload.note_fallback ?? false);
    if (payload.error) setError(payload.error);
    if (payload.note_id) { setCreatedNote(payload.note_id); void refreshAi(); }
  };
  useEffect(() => {
    let stopDelta: (() => void) | undefined; let stopComplete: (() => void) | undefined;
    void listen<{ request_id: string; text: string }>("analytics://delta", event => {
      if (event.payload.request_id === activeRequestId.current) { setAnswer(current => current + event.payload.text); return; }
      const pending = pendingEvents.current.get(event.payload.request_id) ?? { deltas: [] };
      pending.deltas.push(event.payload.text); pendingEvents.current.set(event.payload.request_id, pending);
    }).then(stop => { stopDelta = stop; }).catch(() => {});
    void listen<{ request_id: string; error?: string; usage?: Usage; note_id?: string; note_fallback?: boolean }>("analytics://complete", event => {
      if (event.payload.request_id === activeRequestId.current) { finishRequest.current(event.payload); return; }
      const pending = pendingEvents.current.get(event.payload.request_id) ?? { deltas: [] };
      pending.complete = event.payload; pendingEvents.current.set(event.payload.request_id, pending);
    }).then(stop => { stopComplete = stop; }).catch(() => {});
    return () => { stopDelta?.(); stopComplete?.(); };
  }, []);

  const rows = useMemo(() => (library?.matches ?? []).filter(entry => filter === "all"
    || (filter === "pinned" && entry.pinned) || (filter === "notAnalysed" && entry.status.state !== "ready")), [library, filter]);
  const selected = library?.matches.find(entry => entry.match_id === selectedId) ?? null;
  useEffect(() => { setOwnHero(selected?.own_hero ?? null); setHero(null); }, [selectedId, selected?.own_hero]);
  const aiAvailable = prefs.consent && prefs.ai_on && (!keyStatus.requires_key || keyStatus.set);
  const state = analyticsViewState({ entries: library?.matches ?? [], selectedId, selectedReady: !!bundle,
    openDocument: openNote !== SYSTEM_NOTE_ID, aiAvailable, showList, showSettings: false });
  const total = library?.matches.reduce((sum, entry) => sum + entry.storage_bytes, 0) ?? 0;
  const analysedCount = library?.matches.filter(entry => entry.status.state === "ready" && !entry.pinned).length ?? 0;
  const matchHeroes = heroRows(bundle);
  const deaths = deathRows(bundle);
  const kills = heroKillCounts(bundle);
  const timeCitations = [...noteText.matchAll(/\[[^\]]+\]\(#t=(\d{1,7})\)/g)].map(match => Number(match[1])).filter(Number.isFinite);

  async function selectEntry(entry: ReplayLibraryEntry) {
    setSelectedId(entry.match_id); setShowList(false); setCreatedNote(null); setNoteFallback(false); setError(null);
    if (!["ready", "parsing", "queued"].includes(entry.status.state)) {
      try { await libraryClient.parse(entry.match_id); void refresh(); }
      catch { setError(t(locale, "เริ่มวิเคราะห์รีเพลย์ไม่ได้", "Could not start replay analysis")); }
    }
  }
  async function pickDem() {
    try { const path = await libraryClient.pick(); if (path) { await libraryClient.parse(path); void refresh(); } }
    catch { setError(t(locale, "เลือกหรือวิเคราะห์ .dem ไม่สำเร็จ", "Could not select or analyse the .dem file")); }
  }
  async function togglePin(entry: ReplayLibraryEntry) {
    try { await libraryClient.pin(entry.match_id, !entry.pinned); void refresh(); }
    catch { setError(t(locale, "บันทึกสถานะปักหมุดไม่ได้", "Could not update pin")); }
  }
  async function ask() {
    if (!question.trim() || streaming || !selectedId) return;
    if (!aiAvailable) {
      if (prefs.ai_on && !prefs.consent) { setConsentOpen(true); return; }
      setAnswer(localTemplate(bundle, question.trim(), ownHero ?? selected?.own_hero ?? null, selected, locale));
      setUsage(null); setQuestion(""); setNoteFallback(false);
      return;
    }
    setError(null); setAnswer(""); setUsage(null); setCreatedNote(null); setNoteFallback(false); setStreaming(true);
    try {
      const id = await invoke<string>("analytics_chat_submit", {
        matchId: selectedId, question: question.trim(), openDocument: includeContext.document ? noteText : "",
        selectedHero: includeContext.hero ? hero : null,
        includeSelectedHero: includeContext.hero, includeOwnHero: includeContext.myHero,
        includeMatch: includeContext.match, includeClock: includeContext.time,
        includeDocument: includeContext.document, scrubberTimeS: currentTime,
      });
      activeRequestId.current = id; setRequestId(id); setQuestion("");
      const pending = pendingEvents.current.get(id); pendingEvents.current.delete(id);
      if (pending?.deltas.length) setAnswer(pending.deltas.join(""));
      if (pending?.complete) finishRequest.current({ request_id: id, ...pending.complete });
    } catch (reason) {
      setStreaming(false);
      const kind = providerErrorKind(reason);
      const providerCopy = AI_PROVIDER_COPY[locale];
      setError(kind === "match_active" ? providerCopy.matchBlocked
        : kind === "local_program_unavailable" ? providerCopy.localUnavailable
          : kind === "endpoint_invalid" ? providerCopy.endpointInvalid
            : kind === "redirect_refused" ? providerCopy.redirectRefused
              : kind === "consent_required" ? providerCopy.consentRequired
                : kind === "key_required" ? providerCopy.keyRequired
                  : typeof reason === "string" ? reason : t(locale, "ส่งคำถามไม่สำเร็จ", "Could not send the question"));
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
      void refreshAi();
    } catch {
      setError(locale === "th" ? "บันทึกโน้ตไม่ได้" : "Could not save the note");
    }
  }
  async function setOwnHeroForEntry(entry: ReplayLibraryEntry) {
    let choices = matchHeroes;
    if (entry.match_id !== selectedId) {
      try {
        const raw = await libraryClient.open(entry.match_id); const result = admitLibraryBundle(raw);
        choices = result.ok ? heroRows(result.bundle) : [];
      } catch { choices = []; }
    }
    const heroNames = choices.map(item => typeof item.id === "string" ? item.id.replace("npc_dota_hero_", "") : "").filter(Boolean);
    const answer = window.prompt(t(locale, `ระบุฮีโร่ของฉัน (${heroNames.join(", ") || UNKNOWN_VALUE})`, `My hero (${heroNames.join(", ") || UNKNOWN_VALUE})`), entry.own_hero?.replace("npc_dota_hero_", "") ?? "");
    if (answer === null || !answer.trim()) return;
    try { const id = await invoke<string>("analytics_own_hero_set", { matchId: entry.match_id, hero: answer.trim() }); if (entry.match_id === selectedId) setOwnHero(id); void refresh(); }
    catch { setError(t(locale, "บันทึกฮีโร่ของฉันไม่ได้", "Could not save your hero")); }
  }
  const identifyOwnHero = useCallback((heroId: string) => {
    setOwnHero(heroId);
    if (!selectedId) return;
    void invoke("analytics_own_hero_set", { matchId: selectedId, hero: heroId }).then(() => { void refresh(); })
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
  useEffect(() => {
    if (!restoreContext) return;
    setSelectedId(restoreContext.matchId);
    setOpenNote(restoreContext.documentId);
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
  function seekFromDocument(event: MouseEvent<HTMLDivElement>) {
    const button = (event.target as Element).closest("button.an-md-seek"); if (!button) return;
    const action = actionFromMarkdownTarget(button); const root = viewerRoot.current;
    if (action?.kind === "time" && root) seek(action.seconds);
    if (action?.kind === "event" && root && action.table === "deaths") root.querySelector<HTMLButtonElement>(`#rv-dlist [data-index="${action.index}"]`)?.click();
  }

  const documentTitle = openNote === SYSTEM_NOTE_ID ? t(locale, "สรุปแมตช์.md", "Match summary.md") : notes.find(note => note.id === openNote)?.title ?? openNote;
  const duration = selected?.duration_s == null ? UNKNOWN_VALUE : clock(selected.duration_s);
  const timelineStart = typeof bundle?.match.t0 === "number" ? bundle.match.t0 : 0;
  const timelineEnd = typeof bundle?.match.t1 === "number" ? bundle.match.t1 : null;
  const durationSeconds = bundle?.durationS ?? null;
  const timelineLength = timelineEnd !== null ? Math.max(1, timelineEnd - timelineStart) : 1;
  const timePercent = (seconds: number) => Math.max(0, Math.min(100, (seconds - timelineStart) / timelineLength * 100));
  const playbackLabel = `${formatAnalyticsTime(currentTime)} / ${formatAnalyticsTime(durationSeconds)}`;
  const heroName = matchHeroes.find(item => item.id === (selected?.own_hero ?? ownHero))?.name;
  const selectedHeroName = typeof heroName === "string" ? heroName
    : (selected?.own_hero ?? ownHero)?.replace("npc_dota_hero_", "") ?? UNKNOWN_VALUE;
  const lineups = bundle ? `Radiant ${matchHeroes.filter(item => item.team === 2).length} | Dire ${matchHeroes.filter(item => item.team === 3).length}` : UNKNOWN_VALUE;
  const result = selected?.local_history?.result === "win" ? t(locale, "ชนะ", "Win")
    : selected?.local_history?.result === "loss" ? t(locale, "แพ้", "Loss") : UNKNOWN_VALUE;
  const localKda = selected?.local_history
    ? `${selected.local_history.kills ?? UNKNOWN_VALUE}/${selected.local_history.deaths ?? UNKNOWN_VALUE}/${selected.local_history.assists ?? UNKNOWN_VALUE}`
    : UNKNOWN_VALUE;
  const heroKillTotal = kills.radiant == null || kills.dire == null ? UNKNOWN_VALUE : String(kills.radiant + kills.dire);
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
            onOpenReplay?.(enterAnalyticsReplayDetails(bundle, { matchId: selectedId, timeS: currentTime, documentId: openNote }));
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
  return <div className="analytics-page" data-state={state} data-left-collapsed={leftCollapsed} data-right-collapsed={rightCollapsed}>
    <div className="analytics-topbar">
      <div className="analytics-replay-select">
        <button type="button" aria-label={t(locale, "รีเพลย์ก่อนหน้า", "Previous replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index - 1 + rows.length) % rows.length]!);
        }}>‹</button>
        <select aria-label={t(locale, "เลือกรีเพลย์", "Choose replay")} value={selectedId ?? ""} onChange={event => {
          const entry = library?.matches.find(item => item.match_id === event.currentTarget.value); if (entry) void selectEntry(entry);
        }}>
          <option value="">{loading ? t(locale, "กำลังโหลด…", "Loading…") : t(locale, "เลือกรีเพลย์", "Choose replay")}</option>
          {(library?.matches ?? []).map(entry => <option key={entry.match_id} value={entry.match_id}>{thaiDate(entry, locale)} · {entry.own_hero?.replace("npc_dota_hero_", "") ?? UNKNOWN_VALUE} · {entry.local_history?.result ?? UNKNOWN_VALUE} · {statusText(entry.status, locale)}</option>)}
        </select>
        <button type="button" aria-label={t(locale, "รีเพลย์ถัดไป", "Next replay")} onClick={() => {
          const index = rows.findIndex(row => row.match_id === selectedId); if (rows.length) void selectEntry(rows[(index + 1) % rows.length]!);
        }}>›</button>
      </div>
      <div className="analytics-row-facts">
        <span>{selected ? statusText(selected.status, locale) : UNKNOWN_VALUE}</span><span>{thaiDate(selected, locale)}</span>
        <span>{selectedHeroName}</span><span>{lineups}</span><span>{result} · {localKda}</span>
        <span>{duration}</span><span>{selected?.bundle_bytes ? `${(selected.bundle_bytes / 1024 / 1024).toFixed(1)} MB` : UNKNOWN_VALUE}</span>
        <span>{heroKillTotal} {t(locale, "hero kills", "hero kills")}</span>
      </div>
      <div className="analytics-top-actions"><div role="group" aria-label={t(locale, "ตัวกรองรีเพลย์", "Replay filters")}>
        <button aria-pressed={filter === "all"} onClick={() => setFilter("all")}>{t(locale, "ทั้งหมด", "All")}</button>
        <button aria-pressed={filter === "pinned"} onClick={() => setFilter("pinned")}>{t(locale, "ปักหมุด", "Pinned")}</button>
        <button aria-pressed={filter === "notAnalysed"} onClick={() => setFilter("notAnalysed")}>{t(locale, "ยังไม่วิเคราะห์", "Not analysed")}</button>
      </div><span className="analytics-storage">{analysedCount}/{prefs.keep_newest} · {(total / 1024 ** 2).toFixed(0)} MB / {(prefs.max_bytes / 1024 ** 3).toFixed(1)} GB</span>
        <button type="button" onClick={() => void pickDem()}>.dem…</button><button type="button" onClick={() => setShowList(true)}>⋯</button>
      </div>
    </div>
    <div className="analytics-columns">
      {!leftCollapsed ? <aside className="analytics-column analytics-docs"><header><button onClick={() => setLeftCollapsed(true)} aria-label={t(locale, "ยุบแผงซ้าย", "Collapse left panel")}>‹</button><strong>{t(locale, "วิเคราะห์", "Analysis")}</strong></header>
        <div className="analytics-doc-tabs" role="tablist" aria-label={t(locale, "เอกสาร", "Documents")}>
          <button role="tab" aria-selected={openNote === SYSTEM_NOTE_ID} onClick={() => setOpenNote(SYSTEM_NOTE_ID)}>{t(locale, "สรุปแมตช์.md", "Match summary.md")}</button>
          {notes.map(note => <button role="tab" key={note.id} aria-selected={openNote === note.id} onClick={() => setOpenNote(note.id)}>{note.ai && <b>AI</b>}{note.title}</button>)}
        </div>
        <div className="analytics-doc-toolbar"><strong title={documentTitle}>{documentTitle}</strong><button onClick={() => void navigator.clipboard?.writeText(noteText)}>{t(locale, "คัดลอก", "Copy")}</button>
          <button onClick={() => setShowRawSource(value => !value)}>{showRawSource ? t(locale, "ดูตัวอย่าง", "Preview") : t(locale, "ดูต้นฉบับ .md", "View source .md")}</button>
          <button onClick={() => selectedId && void invoke("analytics_note_open_folder", { matchId: selectedId })}>{t(locale, "เปิดโฟลเดอร์", "Open folder")}</button>
          {openNote !== SYSTEM_NOTE_ID && notes.some(note => note.id === openNote && note.ai) && <button onClick={() => {
            if (!selectedId || !window.confirm(t(locale, "ลบโน้ต AI นี้หรือไม่?", "Delete this AI note?"))) return;
            void invoke("analytics_note_delete", { matchId: selectedId, noteId: openNote }).then(() => { setOpenNote(SYSTEM_NOTE_ID); void refreshAi(); });
          }}>{t(locale, "ลบโน้ต", "Delete note")}</button>}
        </div>
        {showRawSource ? <pre className="analytics-markdown analytics-source">{noteText}</pre> : <div className="analytics-markdown" onClick={seekFromDocument} dangerouslySetInnerHTML={{ __html: markdown }} />}
      </aside> : <button className="analytics-expand" onClick={() => setLeftCollapsed(false)}>›</button>}
      <main className="analytics-column analytics-centre">
        {!bundle && <div className="analytics-empty" role="status">{state === "empty" ? t(locale, "ยังไม่มีรีเพลย์ในคลัง", "No replays in the library") : state === "parsing" ? t(locale, "กำลังวิเคราะห์รีเพลย์…", "Analysing replay…") : t(locale, "เลือกรีเพลย์ที่พร้อม", "Select a ready replay")}</div>}
        {bundle && <div className="analytics-kill-score" aria-label={t(locale, "hero kills ต่อทีม", "Hero kills by team")}>
          <span>{t(locale, "Radiant", "Radiant")} {kills.radiant ?? UNKNOWN_VALUE}</span><b>{heroKillTotal}</b><span>{t(locale, "Dire", "Dire")} {kills.dire ?? UNKNOWN_VALUE}</span>
        </div>}
        {bundle && <MatchViewerSurface locale={locale} bundle={bundle} onHeroSelect={setHero} onHeroIdentify={identifyOwnHero}
          onTimeChange={onTimeChange} onReady={onViewerReady} initialTime={0} disableKeyboard />}
        {player}
      </main>
      {!rightCollapsed ? <aside className="analytics-column analytics-chat"><header><strong>{t(locale, "ผู้ช่วย", "Assistant")}</strong><button onClick={() => setRightCollapsed(true)} aria-label={t(locale, "ยุบแผงขวา", "Collapse right panel")}>›</button></header>
        <ProviderControls locale={locale} settings={prefs} onSettingsChange={setPrefs} onKeyStatusChange={setKeyStatus} compact consentOpen={consentOpen} onConsentOpenChange={setConsentOpen} />
        <div className="analytics-chat-feed" aria-live="polite">
          {state === "noai" && <div className="analytics-noai"><strong>{t(locale, "เปิดใช้ผู้ให้บริการ AI ในการตั้งค่า", "Enable an AI provider in Settings")}</strong><p>{t(locale, "ถามด้วยแม่แบบในเครื่องได้ คำถามเหล่านี้ไม่ส่งข้อมูลออกจากเครื่อง", "Use local templates; these questions do not send data off-device.")}</p>
            <div className="analytics-template-questions">{ANALYTICS_TEMPLATE_QUESTIONS.map(item => <button key={item.id} onClick={() => setAnswer(localTemplate(bundle, locale === "th" ? item.th : item.en, ownHero ?? selected?.own_hero ?? null, selected, locale))}>{locale === "th" ? item.th : item.en}</button>)}</div>
          </div>}
          {answer && <article className="analytics-answer"><p>{answer}</p>{usage && <small>{t(locale, "อินพุต", "Input")} {usage.input ?? UNKNOWN_VALUE} · cache {usage.cache_read ?? UNKNOWN_VALUE} · {t(locale, "เอาต์พุต", "Output")} {usage.output ?? UNKNOWN_VALUE}</small>}</article>}
          {shouldOfferSaveAsNote({ answer, savedNoteId: createdNote, noteFallback }) && <button type="button" onClick={() => void saveAnswerAsNote()}>{AI_PROVIDER_COPY[locale].noteFallback}</button>}
          {createdNote && <button className="analytics-created-note" onClick={() => { setOpenNote(createdNote); setCreatedNote(null); }}>{t(locale, "สร้างแล้ว · เปิดในแผงซ้าย", "Created · open in left panel")}: {createdNote}</button>}
          {error && <p className="analytics-error" role="alert">{error}</p>}
        </div>
        <div className="analytics-context"><span>{t(locale, "ผู้ช่วยเห็น:", "Assistant sees:")}</span>
          {(["match", "time", "document", "myHero", "hero"] as const).map((chip, i) => <button key={chip} aria-pressed={includeContext[chip]} onClick={() => setIncludeContext(old => ({ ...old, [chip]: !old[chip] }))}>{t(locale, ["แมตช์", "เวลา", "เอกสาร", "ฮีโร่ฉัน", "ฮีโร่ที่เลือก"][i]!, ["match", "time", "document", "my hero", "selected hero"][i]!)} ×</button>)}
        </div>
        <form className="analytics-composer" onSubmit={event => { event.preventDefault(); void ask(); }}>
          {!prefs.consent && <button type="button" onClick={() => setConsentOpen(true)}>{AI_PROVIDER_COPY[locale].consent}</button>}
          <textarea value={question} onChange={event => setQuestion(event.currentTarget.value)} placeholder={t(locale, "ถามเกี่ยวกับแมตช์นี้…", "Ask about this match…")} disabled={streaming || !bundle} />
          <div><span>{t(locale, "เมื่อ AI เปิดและคุณส่งคำถาม ข้อมูลจะส่งไปยังผู้ให้บริการที่เลือก", "When AI is on and you submit, data goes to the selected provider")}</span>
            {streaming ? <button type="button" onClick={() => { if (requestId) void invoke("analytics_chat_cancel", { requestId }); setStreaming(false); }}>{t(locale, "ยกเลิก", "Cancel")}</button> : <button type="submit" disabled={!question.trim() || !bundle}>{t(locale, "ส่ง", "Send")}</button>}
          </div>
        </form>
        <footer>{prefs.provider} · {prefs.model || UNKNOWN_VALUE} · {streaming ? t(locale, "กำลังตอบ…", "Responding…") : usage ? `${usage.input ?? UNKNOWN_VALUE} in · ${usage.cache_read ?? UNKNOWN_VALUE} cache read · ${usage.output ?? UNKNOWN_VALUE} out` : t(locale, "ไม่มีข้อมูลโทเคน", "No token data")}</footer>
      </aside> : <button className="analytics-expand" onClick={() => setRightCollapsed(false)}>‹</button>}
    </div>
    {showList && <div className="analytics-list-overlay" role="dialog" aria-modal="true" aria-label={t(locale, "คลังรีเพลย์", "Replay library")}><section>
      <header><strong>{t(locale, "คลังรีเพลย์", "Replay library")}</strong><button onClick={() => setShowList(false)}>×</button></header>
      {rows.map(entry => <article key={entry.match_id}><button onClick={() => void selectEntry(entry)}>{statusText(entry.status, locale)} · {entry.match_date_utc ?? UNKNOWN_VALUE} · {entry.own_hero ?? UNKNOWN_VALUE}</button>
        <button onClick={() => void togglePin(entry)}>{entry.pinned ? t(locale, "เลิกปักหมุด", "Unpin") : t(locale, "เก็บไว้", "Keep")}</button>
        <button onClick={() => void setOwnHeroForEntry(entry)}>{t(locale, "ระบุฮีโร่ของฉัน", "Set my hero")}</button><button onClick={() => void libraryClient.parse(entry.match_id)}>{t(locale, "วิเคราะห์ใหม่", "Re-analyse")}</button>
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
