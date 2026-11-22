export type DraftMode = "hidden" | "consent_chip" | "suggestions" | "pick_card";
export type DraftHiddenReason = "unsupported_output" | "bar_off" | "no_draft" | "list_build_disabled";
export type DraftDataState = "consent_off" | "loading" | "live" | "stale" | "fetch_failed" | "rate_limited" | "provider_error" | "bracket_empty";
export type DraftPhase = "hero_selection" | "strategy_time";
export type DraftFailure = "network" | "timeout" | "http_status" | "invalid_body" | "too_large" | "rate_limited" | "cache_delete_failed";
export type PickCardState = "live" | "stale" | "loading" | "unknown_hero" | "not_enough_matches" | "fetch_failed" | "rate_limited" | "provider_error" | "bracket_empty";
export type WinRateBand = "green" | "neutral" | "rose";

export interface DraftColumnRow {
  hero_id: number;
  name: string;
  rank: number | null;
  games: number | null;
  win_rate_tenths: number | null;
  low_data: boolean;
  taken: boolean;
}

export type DraftCounterState = "build_disabled" | "draft_bar_unavailable" | "sides_unknown" | "no_enemy" | "ready";

export interface DraftCounterRow {
  hero_id: number;
  name: string;
  against: string[];
  reason: string;
}

export interface DraftCounters {
  state: DraftCounterState;
  source: "kohbor_reviewed_table";
  enemies: string[];
  enemies_identified: number;
  position_filtered: boolean;
  rows: DraftCounterRow[];
}

export interface DraftDurationCell {
  bin: "short" | "mid" | "long";
  games: number | null;
  win_rate_tenths: number | null;
}

export interface DraftPickCard {
  hero_id: number | null;
  name: string | null;
  state: PickCardState;
  games: number | null;
  win_rate_tenths: number | null;
  form_tenths: number | null;
  durations: {
    state: PickCardState;
    fetched_at_ms: number | null;
    cells: DraftDurationCell[];
  };
}

export interface DraftColumnView {
  valid: boolean;
  mode: DraftMode;
  overlay: { visible: boolean; hidden_reason: DraftHiddenReason | null };
  phase: DraftPhase | null;
  placement_verified: boolean;
  data_state: DraftDataState | null;
  context: { position: number | null; bracket: number | null };
  source: {
    provider: "OpenDota";
    window: string | null;
    fetched_at_ms: number | null;
    failed_at_ms: number | null;
    failure: DraftFailure | null;
    http_status: number | null;
    retry_after_ms: number | null;
    table_requests_24h: number;
    table_request_budget: 4;
  } | null;
  suggestions: {
    rows: DraftColumnRow[];
    ranked_count: number;
    draft_bar: { available: boolean; identified: number; slots: 10 };
    /** Null when the host sent no counter block (an older host) or the block was invalid. */
    counters: DraftCounters | null;
  } | null;
  pick_card: DraftPickCard | null;
}

export interface DraftStatsPrefs {
  hero_stats_consent: boolean;
  draft_bar_enabled: boolean;
  position: number | null;
  bracket: number | null;
}

export interface DraftRetryBudget {
  used: number | null;
  limit: number;
  label: string;
  spent: boolean;
}

export interface DraftColumnPlacement {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

const MODES: readonly DraftMode[] = ["hidden", "consent_chip", "suggestions", "pick_card"];
const HIDDEN_REASONS: readonly DraftHiddenReason[] = ["unsupported_output", "bar_off", "no_draft", "list_build_disabled"];
const DATA_STATES: readonly DraftDataState[] = ["consent_off", "loading", "live", "stale", "fetch_failed", "rate_limited", "provider_error", "bracket_empty"];
const PHASES: readonly DraftPhase[] = ["hero_selection", "strategy_time"];
const FAILURES: readonly DraftFailure[] = ["network", "timeout", "http_status", "invalid_body", "too_large", "rate_limited", "cache_delete_failed"];
const PICK_CARD_STATES: readonly PickCardState[] = ["live", "stale", "loading", "unknown_hero", "not_enough_matches", "fetch_failed", "rate_limited", "provider_error", "bracket_empty"];
const COUNTER_STATES: readonly DraftCounterState[] = ["build_disabled", "draft_bar_unavailable", "sides_unknown", "no_enemy", "ready"];
/** The overlay draws at most this many counter rows on top of the meta list. */
export const OVERLAY_COUNTER_ROWS = 3;
const MAX_COUNTER_ROWS = 6;
const BRACKETS = ["Herald", "Guardian", "Crusader", "Archon", "Legend", "Ancient", "Divine", "Immortal"] as const;
const BINS = ["short", "mid", "long"] as const;
const MAX_STAT_COUNT = 1_000_000_000;
const MAX_TIMESTAMP_MS = 8_640_000_000_000_000;
const MAX_RETRY_MS = 7 * 24 * 60 * 60 * 1000;

/** Largest column rect (Option A, ends at y 860); strategy-time clearance checked only on two owner frames. */
export const PICK_CARD_STRATEGY_TIME_RECT: DraftColumnPlacement = {
  x: 1728, y: 132, width: 180, height: 728, scale: 1,
};
export const PICK_CARD_STRATEGY_TIME_RECT_VERIFIED = false;

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function integer(value: unknown, min: number, max: number): number | null {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max ? value : null;
}

function strictNullableInteger(value: unknown, min: number, max: number): number | null | undefined {
  if (value === null) return null;
  return integer(value, min, max) ?? undefined;
}

function boundedText(value: unknown, maxLength: number): string | null {
  return typeof value === "string" && value.trim().length > 0 && value.length <= maxLength ? value.trim() : null;
}

function isOneOf<T extends string>(value: unknown, values: readonly T[]): value is T {
  return typeof value === "string" && values.includes(value as T);
}

function emptyHiddenView(): DraftColumnView {
  return {
    valid: false,
    mode: "hidden",
    overlay: { visible: false, hidden_reason: null },
    phase: null,
    placement_verified: false,
    data_state: null,
    context: { position: null, bracket: null },
    source: null,
    suggestions: null,
    pick_card: null,
  };
}

export const HIDDEN_DRAFT_COLUMN: DraftColumnView = emptyHiddenView();

export function parseDraftStatsPrefs(value: unknown): DraftStatsPrefs | null {
  const prefs = record(value);
  if (!prefs || typeof prefs.hero_stats_consent !== "boolean" || typeof prefs.draft_bar_enabled !== "boolean") return null;
  if (prefs.position !== null && integer(prefs.position, 1, 5) === null) return null;
  if (prefs.bracket !== null && integer(prefs.bracket, 1, 8) === null) return null;
  return {
    hero_stats_consent: prefs.hero_stats_consent,
    draft_bar_enabled: prefs.draft_bar_enabled,
    position: prefs.position === null ? null : integer(prefs.position, 1, 5),
    bracket: prefs.bracket === null ? null : integer(prefs.bracket, 1, 8),
  };
}

export function retryBudgetView(view: DraftColumnView | null): DraftRetryBudget {
  const used = view?.source?.table_requests_24h ?? null;
  const limit = 4;
  return {
    used,
    limit,
    label: `${used ?? "—"}/${limit} ใน 24 ชม.`,
    spent: used !== null && used >= limit,
  };
}

function parseSource(value: unknown): DraftColumnView["source"] {
  const source = record(value);
  if (!source || source.provider !== "OpenDota" || source.window !== null) return null;
  const failure = source.failure === null ? null : isOneOf(source.failure, FAILURES) ? source.failure : undefined;
  const fetched = strictNullableInteger(source.fetched_at_ms, 0, MAX_TIMESTAMP_MS);
  const failed = strictNullableInteger(source.failed_at_ms, 0, MAX_TIMESTAMP_MS);
  const status = strictNullableInteger(source.http_status, 100, 599);
  const retry = strictNullableInteger(source.retry_after_ms, 0, MAX_RETRY_MS);
  const used = integer(source.table_requests_24h, 0, 4);
  if (source.table_request_budget !== 4) return null;
  if (failure === undefined || fetched === undefined || failed === undefined || status === undefined || retry === undefined || used === null) return null;
  return {
    provider: "OpenDota",
    window: null,
    fetched_at_ms: fetched,
    failed_at_ms: failed,
    failure,
    http_status: status,
    retry_after_ms: retry,
    table_requests_24h: used,
    table_request_budget: 4,
  };
}

function parseRow(value: unknown): DraftColumnRow | null {
  const row = record(value);
  if (!row || typeof row.low_data !== "boolean" || typeof row.taken !== "boolean") return null;
  const heroId = integer(row.hero_id, 0, 1_000_000);
  const name = boundedText(row.name, 96);
  const rank = strictNullableInteger(row.rank, 1, 127);
  const games = integer(row.games, 0, MAX_STAT_COUNT);
  const rate = strictNullableInteger(row.win_rate_tenths, 0, 1000);
  if (heroId === null || name === null || rank === undefined || games === null || rate === undefined) return null;
  return {
    hero_id: heroId,
    name,
    rank,
    games,
    win_rate_tenths: rate,
    low_data: row.low_data,
    taken: row.taken,
  };
}

function parseDraftBar(value: unknown): NonNullable<DraftColumnView["suggestions"]>["draft_bar"] | null {
  const draftBar = record(value);
  if (!draftBar || typeof draftBar.available !== "boolean" || draftBar.slots !== 10) return null;
  const identified = integer(draftBar.identified, 0, 10);
  if (identified === null) return null;
  return { available: draftBar.available, identified, slots: 10 };
}

function parseNameList(value: unknown, max: number): string[] | null {
  if (!Array.isArray(value) || value.length > max) return null;
  const names: string[] = [];
  for (const item of value) {
    const name = boundedText(item, 96);
    if (name === null) return null;
    names.push(name);
  }
  return names;
}

/** A counter block that violates any type, enum or bound is dropped whole (no counters shown). */
export function parseCounters(value: unknown): DraftCounters | null {
  const counters = record(value);
  if (!counters || !isOneOf(counters.state, COUNTER_STATES) || counters.source !== "kohbor_reviewed_table") return null;
  if (typeof counters.position_filtered !== "boolean" || !Array.isArray(counters.rows) || counters.rows.length > MAX_COUNTER_ROWS) return null;
  const enemies = parseNameList(counters.enemies, 5);
  const identified = integer(counters.enemies_identified, 0, 5);
  if (enemies === null || identified === null) return null;
  const rows: DraftCounterRow[] = [];
  for (const item of counters.rows) {
    const row = record(item);
    if (!row) return null;
    const heroId = integer(row.hero_id, 0, 1_000_000);
    const name = boundedText(row.name, 96);
    const against = parseNameList(row.against, 5);
    const reason = boundedText(row.reason, 240);
    if (heroId === null || name === null || against === null || against.length === 0 || reason === null) return null;
    rows.push({ hero_id: heroId, name, against, reason });
  }
  if (counters.state !== "ready" && (rows.length > 0 || enemies.length > 0)) return null;
  return {
    state: counters.state,
    source: "kohbor_reviewed_table",
    enemies,
    enemies_identified: identified,
    position_filtered: counters.position_filtered,
    rows,
  };
}

/** Counter rows to draw: only in the ready state, at most three in game. */
export function visibleCounterRows(counters: DraftCounters | null, surface: "overlay" | "deck"): DraftCounterRow[] {
  if (!counters || counters.state !== "ready") return [];
  return surface === "overlay" ? counters.rows.slice(0, OVERLAY_COUNTER_ROWS) : counters.rows;
}

export function counterHeading(counters: DraftCounters): string {
  return `แก้ทาง ${counters.enemies.join(" · ")}`;
}

export function counterStateText(counters: DraftCounters | null): string | null {
  if (!counters) return null;
  switch (counters.state) {
    case "build_disabled": return "แนะนำตัวแก้ทางยังปิดอยู่ · รอเจ้าของตรวจตาราง";
    case "draft_bar_unavailable": return "แก้ทาง: Kohbor ยังไม่เห็นฮีโร่ในแถบดราฟต์";
    case "sides_unknown": return "แก้ทาง: ยังไม่รู้ว่าฮีโร่ไหนอยู่ฝั่งตรงข้าม";
    case "no_enemy": return "แก้ทาง: ยังไม่เห็นฮีโร่ฝั่งตรงข้าม";
    case "ready": return counters.rows.length === 0 ? "แก้ทาง: ตารางยังไม่มีคำแนะนำสำหรับฮีโร่ที่เห็น" : null;
  }
}

function parseSuggestions(value: unknown): DraftColumnView["suggestions"] {
  if (value === null) return null;
  const suggestions = record(value);
  if (!suggestions || !Array.isArray(suggestions.rows) || suggestions.rows.length > 16) return null;
  const rankedCount = integer(suggestions.ranked_count, 0, MAX_STAT_COUNT);
  const draftBar = parseDraftBar(suggestions.draft_bar);
  if (rankedCount === null || draftBar === null) return null;
  const rows: DraftColumnRow[] = [];
  for (const value of suggestions.rows) {
    const row = parseRow(value);
    if (row === null) return null;
    rows.push(row);
  }
  return {
    rows,
    ranked_count: rankedCount,
    draft_bar: draftBar,
    counters: parseCounters(suggestions.counters),
  };
}

function parseDurationCell(value: unknown, expectedBin: typeof BINS[number]): DraftDurationCell | null {
  const cell = record(value);
  if (!cell || cell.bin !== expectedBin) return null;
  const games = strictNullableInteger(cell.games, 0, MAX_STAT_COUNT);
  const rate = strictNullableInteger(cell.win_rate_tenths, 0, 1000);
  if (games === undefined || rate === undefined) return null;
  return {
    bin: expectedBin,
    games,
    win_rate_tenths: rate,
  };
}

function parsePickCard(value: unknown): DraftPickCard | null {
  if (value === null) return null;
  const card = record(value);
  if (!card || !isOneOf(card.state, PICK_CARD_STATES)) return null;
  const durations = record(card.durations);
  if (!durations || !isOneOf(durations.state, PICK_CARD_STATES) || !Array.isArray(durations.cells) || durations.cells.length !== 3) return null;
  const durationCells = durations.cells as unknown[];
  const cells = BINS.map((bin, index) => parseDurationCell(durationCells[index], bin));
  if (cells.some((cell) => cell === null)) return null;
  const heroId = card.hero_id === null ? null : integer(card.hero_id, 0, 1_000_000);
  const name = card.name === null ? null : boundedText(card.name, 96);
  if ((card.hero_id !== null && heroId === null) || (card.name !== null && name === null)) return null;
  const games = strictNullableInteger(card.games, 0, MAX_STAT_COUNT);
  const rate = strictNullableInteger(card.win_rate_tenths, 0, 1000);
  const form = strictNullableInteger(card.form_tenths, -1000, 1000);
  const fetched = strictNullableInteger(durations.fetched_at_ms, 0, MAX_TIMESTAMP_MS);
  if (games === undefined || rate === undefined || form === undefined || fetched === undefined) return null;
  return {
    hero_id: heroId,
    name,
    state: card.state,
    games,
    win_rate_tenths: rate,
    form_tenths: form,
    durations: {
      state: durations.state,
      fetched_at_ms: fetched,
      cells: cells as DraftDurationCell[],
    },
  };
}

/** Parse the host-owned schema 1 view. Unknown structure and enums hide the renderer. */
export function parseDraftColumnView(payload: unknown): DraftColumnView {
  const input = record(payload);
  if (!input || input.schema !== 1 || !isOneOf(input.mode, MODES) || !isOneOf(input.data_state, DATA_STATES) ||
      typeof input.placement_verified !== "boolean") return emptyHiddenView();

  const overlayInput = record(input.overlay);
  const contextInput = record(input.context);
  if (!overlayInput || typeof overlayInput.visible !== "boolean" || !contextInput) return emptyHiddenView();
  const hiddenReason = overlayInput.hidden_reason === null
    ? null
    : isOneOf(overlayInput.hidden_reason, HIDDEN_REASONS) ? overlayInput.hidden_reason : undefined;
  if (hiddenReason === undefined || (overlayInput.visible && hiddenReason !== null)) return emptyHiddenView();

  const phase = input.phase === null ? null : isOneOf(input.phase, PHASES) ? input.phase : undefined;
  if (phase === undefined) return emptyHiddenView();
  const source = parseSource(input.source);
  if (source === null) return emptyHiddenView();
  const position = strictNullableInteger(contextInput.position, 1, 5);
  const bracket = strictNullableInteger(contextInput.bracket, 1, 8);
  if (position === undefined || bracket === undefined) return emptyHiddenView();

  const mode = input.mode;
  if (mode === "consent_chip" && input.data_state !== "consent_off") return emptyHiddenView();
  const suggestions = parseSuggestions(input.suggestions);
  if (suggestions === null && input.suggestions !== null) return emptyHiddenView();
  const pickCard = parsePickCard(input.pick_card);
  if (pickCard === null && input.pick_card !== null) return emptyHiddenView();

  return {
    valid: true,
    mode,
    overlay: { visible: overlayInput.visible, hidden_reason: hiddenReason },
    phase,
    placement_verified: input.placement_verified,
    data_state: input.data_state,
    context: {
      position,
      bracket,
    },
    source,
    suggestions,
    pick_card: pickCard,
  };
}

export function formatWinRateTenths(value: number | null): string {
  if (value === null || integer(value, 0, 1000) === null) return "—";
  return `${(value / 10).toFixed(1)}%`;
}

export function formatFormTenths(value: number | null): string {
  if (value === null || integer(value, -1000, 1000) === null) return "—";
  const formatted = (Math.abs(value) / 10).toFixed(1);
  return value > 0 ? `+${formatted}` : value < 0 ? `−${formatted}` : "0.0";
}

export function formatGames(value: number | null): string {
  if (value === null || integer(value, 0, MAX_STAT_COUNT) === null) return "—";
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(value);
}

export function winRateBand(value: number | null): WinRateBand {
  if (value === null || integer(value, 0, 1000) === null) return "neutral";
  if (value >= 510) return "green";
  if (value <= 490) return "rose";
  return "neutral";
}

export function suggestionRowView(row: DraftColumnRow) {
  const unknownGames = row.games === null;
  const lowData = row.games !== null && (row.low_data || row.games < 1000);
  const rate = lowData || unknownGames ? null : row.win_rate_tenths;
  return {
    rank: row.taken || lowData || unknownGames ? null : row.rank,
    taken: row.taken,
    gamesText: formatGames(row.games),
    winRateText: formatWinRateTenths(rate),
    band: winRateBand(rate),
    lowDataLabel: lowData ? "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม" : null,
    unknownGamesLabel: unknownGames ? "ไม่ทราบจำนวนเกม" : null,
  };
}

/**
 * Overlay Option A (owner choice 2026-11-22): sectioned cards with fixed block heights, so the host
 * sizes the native window from the view alone. Mirrored in Rust by `hero_stats::layout`; change both.
 * Top-left is x 1728, y 132 at 1920×1080; the column ends no lower than y 860.
 */
export const OVERLAY_LAYOUT = {
  top: 132,
  maxBottom: 860,
  width: 180,
  gap: 6,
  brand: 30,
  cardBorder: 2,
  head: 50,
  counterHead: 70,
  metaRow: 50,
  counterRow: 62,
  message: 88,
  pickBody: 178,
  durationBody: 140,
  footer: 84,
  chip: 56,
  minTextPx: 12,
} as const;

export function overlayValuesAvailable(view: DraftColumnView): boolean {
  return (view.data_state === "live" || view.data_state === "stale") && view.source?.fetched_at_ms !== null
    && view.source?.fetched_at_ms !== undefined;
}

/** Counter rows and meta rows the overlay draws for a suggestions view. */
export function overlaySections(view: DraftColumnView): { counters: DraftCounterRow[]; rows: DraftColumnRow[] } {
  const counters = visibleCounterRows(view.suggestions?.counters ?? null, "overlay");
  const hostRows = overlayValuesAvailable(view) ? view.suggestions?.rows ?? [] : [];
  return { counters, rows: overlaySuggestionRows(hostRows, 8 - counters.length) };
}

/** Exact overlay content height in CSS px at scale 1; 0 when the overlay draws nothing. */
export function overlayColumnHeight(view: DraftColumnView): number {
  const L = OVERLAY_LAYOUT;
  if (!view.valid || !view.overlay.visible || view.mode === "hidden") return 0;
  if (view.mode === "consent_chip") return L.chip;
  if (view.mode === "pick_card") {
    return L.brand + L.gap + (L.cardBorder + L.head + L.pickBody) + L.gap
      + (L.cardBorder + L.head + L.durationBody) + L.gap + L.footer;
  }
  const { counters, rows } = overlaySections(view);
  const counterCard = counters.length > 0 ? L.cardBorder + L.counterHead + counters.length * L.counterRow + L.gap : 0;
  const metaBody = rows.length > 0 ? rows.length * L.metaRow : L.message;
  return L.brand + L.gap + counterCard + (L.cardBorder + L.head + metaBody) + L.gap + L.footer;
}

/** Short status pill for the overlay brand bar; the full state text sits in the section. */
export function overlayPill(state: DraftDataState | PickCardState | null, lowData = false): { text: string; tone: "ok" | "warn" | "err" } {
  if (lowData) return { text: "ข้อมูลน้อย", tone: "warn" };
  switch (state) {
    case "live": return { text: "สด", tone: "ok" };
    case "stale": return { text: "ข้อมูลเก่า", tone: "warn" };
    case "loading": return { text: "กำลังโหลด", tone: "ok" };
    case "not_enough_matches": return { text: "ข้อมูลน้อย", tone: "warn" };
    case "unknown_hero": return { text: "ไม่มีสถิติ", tone: "warn" };
    case "rate_limited": return { text: "ถึงโควตา", tone: "warn" };
    case "bracket_empty": return { text: "ไม่มีข้อมูล", tone: "warn" };
    case "fetch_failed": return { text: "ดึงไม่ได้", tone: "err" };
    case "provider_error": return { text: "ผิดปกติ", tone: "err" };
    case "consent_off": return { text: "ปิดอยู่", tone: "warn" };
    default: return { text: "—", tone: "warn" };
  }
}

/** Prioritize pickable rows; a taken row may fill a spare slot only when it outranks a drawn pickable row. */
export function overlaySuggestionRows(rows: readonly DraftColumnRow[], limit = 8): DraftColumnRow[] {
  const slots = Math.max(0, Math.min(8, limit));
  const retainedUntaken = rows.filter((row) => !row.taken).slice(0, slots);
  const remainingSlots = slots - retainedUntaken.length;
  const retainedTaken = remainingSlots > 0
    ? rows.filter((row) => row.taken && row.rank !== null && retainedUntaken.some((pickable) => (
      pickable.rank !== null && row.rank! < pickable.rank
    ))).slice(0, remainingSlots)
    : [];
  const drawn = new Set([...retainedUntaken, ...retainedTaken]);
  return rows.filter((row) => drawn.has(row));
}

export function positionLabel(position: number | null): string {
  return position === null ? "ทุกตำแหน่ง" : integer(position, 1, 5) === null ? "—" : `ตำแหน่ง ${position} (ค่าประมาณ)`;
}

export function bracketLabel(bracket: number | null): string {
  if (bracket === null) return "ทุกแรงก์";
  return integer(bracket, 1, 8) === null ? "—" : BRACKETS[bracket - 1]!;
}

export function unsetContextLabel(context: { position: number | null; bracket: number | null }): string | null {
  return context.position === null && context.bracket === null ? "ยังไม่ได้ตั้งตำแหน่งและแรงก์" : null;
}

export function dataStateText(view: DraftColumnView): string | null {
  switch (view.data_state) {
    case "consent_off": return "คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor";
    case "loading": return "กำลังขอสถิติจาก OpenDota…";
    case "live": return "สด";
    case "stale": return "ข้อมูลเก่า";
    case "fetch_failed": return "ดึงข้อมูลไม่ได้";
    case "rate_limited": return "ถึงโควตาของ OpenDota แล้ว";
    case "provider_error": return "ผู้ให้บริการสถิติตอบกลับผิดปกติ";
    case "bracket_empty": return view.context.bracket === 8
      ? "OpenDota ไม่มีสถิติของแรงก์ Immortal ตอนนี้"
      : "OpenDota ไม่มีสถิติของแรงก์ที่เลือกตอนนี้";
    default: return null;
  }
}

const FAILURE_TEXT: Record<DraftFailure, string> = {
  network: "เชื่อมต่อ OpenDota ไม่ได้",
  timeout: "OpenDota ไม่ตอบภายในเวลาที่กำหนด",
  http_status: "OpenDota ตอบกลับสถานะ HTTP",
  invalid_body: "ข้อมูลจาก OpenDota ไม่ถูกต้อง",
  too_large: "ข้อมูลจาก OpenDota มีขนาดเกินกำหนด",
  rate_limited: "ถึงโควตาของ OpenDota แล้ว",
  cache_delete_failed: "ลบแคชสถิติไม่สำเร็จ",
};

export function failureText(failure: DraftFailure | null, httpStatus: number | null = null): string | null {
  if (failure === null) return null;
  const text = FAILURE_TEXT[failure];
  return failure === "http_status" ? `${text} ${httpStatus === null ? "—" : httpStatus}` : text;
}

export function formatDraftTime(value: number | null): string {
  if (value === null || integer(value, 0, MAX_TIMESTAMP_MS) === null) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  const options: Intl.DateTimeFormatOptions = {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    numberingSystem: "latn",
  };
  if (date.getFullYear() !== new Date().getFullYear()) options.year = "numeric";
  return new Intl.DateTimeFormat("th-TH-u-ca-gregory", options).format(date);
}

export function staleTimesText(view: DraftColumnView): string {
  return `ข้อมูลของ ${formatDraftTime(view.source?.fetched_at_ms ?? null)} · ดึงใหม่ไม่ได้เมื่อ ${formatDraftTime(view.source?.failed_at_ms ?? null)}`;
}

export function retryDeadlineMs(view: DraftColumnView): number | null {
  const failedAt = view.source?.failed_at_ms ?? null;
  const retryAfter = view.source?.retry_after_ms ?? null;
  if (failedAt === null || retryAfter === null) return null;
  return integer(failedAt + retryAfter, 0, MAX_TIMESTAMP_MS);
}

export function dataStateDetailText(view: DraftColumnView): string | null {
  const source = view.source;
  switch (view.data_state) {
    case "stale": {
      const failure = failureText(source?.failure ?? null, source?.http_status ?? null);
      return `${staleTimesText(view)}${failure ? ` · ${failure}` : ""}`;
    }
    case "fetch_failed": {
      const failure = failureText(source?.failure ?? null, source?.http_status ?? null);
      return `${failure ?? "—"} · ${formatDraftTime(source?.failed_at_ms ?? null)} · ไม่มีสถิติที่เก็บไว้ในเครื่อง`;
    }
    case "rate_limited": {
      return `ลองใหม่ได้หลัง ${formatDraftTime(retryDeadlineMs(view))}`;
    }
    case "provider_error": {
      const status = source?.http_status === null || source?.http_status === undefined ? "—" : source.http_status;
      return `HTTP ${status} · ${formatDraftTime(source?.failed_at_ms ?? null)}`;
    }
    case "bracket_empty": return "เลือกแรงก์อื่นได้ที่หน้าต่าง Kohbor";
    case "loading":
    case "live":
    case "consent_off":
    default: return null;
  }
}

export function pickCardStateText(state: PickCardState): string {
  switch (state) {
    case "live": return "สด";
    case "stale": return "ข้อมูลเก่า";
    case "loading": return "กำลังขอสถิติจาก OpenDota…";
    case "unknown_hero": return "ยังไม่มีสถิติของฮีโร่ตัวนี้";
    case "not_enough_matches": return "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม";
    case "fetch_failed": return "ดึงข้อมูลไม่ได้";
    case "rate_limited": return "ถึงโควตาของ OpenDota แล้ว";
    case "provider_error": return "ผู้ให้บริการสถิติตอบกลับผิดปกติ";
    case "bracket_empty": return "OpenDota ไม่มีสถิติของแรงก์ที่เลือกตอนนี้";
  }
}

export function draftHiddenReasonText(reason: DraftHiddenReason | null): string | null {
  switch (reason) {
    case "unsupported_output": return "แถบในเกมรองรับ 1920×1080 ที่ 100% เท่านั้นในตอนนี้";
    case "list_build_disabled": return "รอเจ้าของตรวจกลุ่มตำแหน่งก่อนเปิดในเกม";
    case "bar_off": return "ปิดแถบดราฟต์ในเกมไว้";
    case "no_draft": return "แสดงสถิติในช่วงดราฟต์เท่านั้น";
    default: return null;
  }
}

/** Deck-only reason when statistics consent is off: the column stays empty until it is on. */
export function draftConsentOffText(dataState: DraftDataState | null): string | null {
  return dataState === "consent_off"
    ? "ยังไม่ได้เปิดการยินยอมสถิติ · เปิดสวิตช์ “ดึงสถิติฮีโร่จากอินเทอร์เน็ต” เพื่อให้แถบฮีโร่เมต้าและตัวแก้ทางแสดงในดราฟต์"
    : null;
}

/**
 * Placement inside the overlay webview. In the app the host sizes the native window to the
 * column rect itself (180×488 at 1728,132 on 1920×1080), so the column fills that window at
 * scale width/180. Only a screen-wide viewport (the browser mock) uses the full-screen
 * formula. Applying the screen formula inside the 180×488 window shrank the column to
 * 0.452× at its right edge (live test on a050ae8, 2026-11-22).
 */
export function draftStagePlacement(width: number, height: number): DraftColumnPlacement | null {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null;
  // The host sizes the window to the column itself: 180 px wide at 1080p, as tall as its content
  // (56–728 px, Option A). A screen-wide viewport is the browser mock.
  if (width < 600) {
    const scale = width / 180;
    return { x: 0, y: 0, width, height, scale };
  }
  return draftColumnPlacement(width, height);
}

export function draftColumnPlacement(width: number, height: number): DraftColumnPlacement | null {
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) return null;
  const scale = height / 1080;
  return {
    x: width - 192 * scale,
    y: 132 * scale,
    width: 180 * scale,
    height: 488 * scale,
    scale,
  };
}
