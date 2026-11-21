type MockInvoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type MockInternals = { invoke: MockInvoke };
type MockRow = { hero_id: number; name: string; rank: number | null; games: number; win_rate_tenths: number | null; low_data: boolean; taken: boolean };
type MockDraftView = {
  schema: 1;
  mode: string;
  overlay: { visible: boolean; hidden_reason: string | null };
  phase: string | null;
  placement_verified: boolean;
  data_state: string;
  context: { position: number | null; bracket: number | null };
  source: {
    provider: "OpenDota"; window: string | null; fetched_at_ms: number | null; failed_at_ms: number | null;
    failure: string | null; http_status: number | null; retry_after_ms: number | null;
    table_requests_24h: number; table_request_budget: number;
  };
  suggestions: { rows: MockRow[]; ranked_count: number; draft_bar: { available: boolean; identified: number; slots: number } } | null;
  pick_card: unknown;
};

const MOCK_ROWS = [
  { hero_id: 1, name: "Spectre", rank: 1, games: 44465, win_rate_tenths: 531, low_data: false, taken: false },
  { hero_id: 2, name: "Phantom Assassin", rank: 2, games: 82337, win_rate_tenths: 528, low_data: false, taken: false },
  { hero_id: 3, name: "Phantom Lancer", rank: 3, games: 67246, win_rate_tenths: 515, low_data: false, taken: false },
  { hero_id: 4, name: "Juggernaut", rank: 4, games: 80501, win_rate_tenths: 512, low_data: false, taken: false },
  { hero_id: 5, name: "Sven", rank: 5, games: 43720, win_rate_tenths: 512, low_data: false, taken: false },
  { hero_id: 6, name: "Bloodseeker", rank: 6, games: 21583, win_rate_tenths: 510, low_data: false, taken: false },
  { hero_id: 7, name: "Anti-Mage", rank: 7, games: 67807, win_rate_tenths: 507, low_data: false, taken: false },
  { hero_id: 8, name: "Luna", rank: 8, games: 52239, win_rate_tenths: 506, low_data: false, taken: false },
];

const mockPrefs = {
  hero_stats_consent: false,
  draft_bar_enabled: true,
  position: 1 as number | null,
  bracket: 3 as number | null,
};

let tableRequestsUsed = 2;

function baseView(): MockDraftView {
  return {
    schema: 1,
    mode: "suggestions",
    overlay: { visible: true, hidden_reason: null },
    phase: "hero_selection",
    placement_verified: true,
    data_state: "live",
    context: { position: mockPrefs.position, bracket: mockPrefs.bracket },
    source: {
      provider: "OpenDota", window: null, fetched_at_ms: 1790000000000, failed_at_ms: null, failure: null,
      http_status: null, retry_after_ms: null, table_requests_24h: tableRequestsUsed, table_request_budget: 4,
    },
    suggestions: {
      rows: MOCK_ROWS.map((row) => ({ ...row })),
      ranked_count: 8,
      draft_bar: { available: true, identified: 2, slots: 10 },
    },
    pick_card: null,
  };
}

function fixtureView(scenario: string) {
  const view = baseView();
  switch (scenario) {
    case "consent_off":
      view.mode = "consent_chip";
      view.data_state = "consent_off";
      view.suggestions = null;
      break;
    case "loading":
      view.data_state = "loading";
      view.suggestions!.rows = [];
      break;
    case "stale":
      view.data_state = "stale";
      view.source.failed_at_ms = 1790003600000;
      view.source.failure = "timeout";
      break;
    case "fetch_failed":
      view.data_state = "fetch_failed";
      view.source.failed_at_ms = 1790003600000;
      view.source.failure = "network";
      view.suggestions!.rows = [];
      break;
    case "rate_limited":
      view.data_state = "rate_limited";
      view.source.failed_at_ms = 1790003600000;
      view.source.failure = "rate_limited";
      view.source.retry_after_ms = 3_600_000;
      view.suggestions!.rows = [];
      break;
    case "provider_error":
      view.data_state = "provider_error";
      view.source.failed_at_ms = 1790003600000;
      view.source.failure = "http_status";
      view.source.http_status = 503;
      view.suggestions!.rows = [];
      break;
    case "bracket_empty":
      view.data_state = "bracket_empty";
      view.context.bracket = 8;
      view.suggestions!.rows = [];
      break;
    case "low_data":
      view.suggestions!.rows = [
        { hero_id: 9, name: "Chen", rank: null, games: 496, win_rate_tenths: null, low_data: true, taken: false },
      ];
      view.suggestions!.ranked_count = 0;
      break;
    case "pick_card":
      view.mode = "pick_card";
      view.phase = "strategy_time";
      view.placement_verified = false;
      view.pick_card = {
        hero_id: 54,
        name: "Lifestealer",
        state: "live",
        games: 48210,
        win_rate_tenths: 505,
        form_tenths: 5,
        durations: {
          state: "live",
          fetched_at_ms: 1790000000000,
          cells: [
            { bin: "short", games: 12400, win_rate_tenths: 540 },
            { bin: "mid", games: 21300, win_rate_tenths: 510 },
            { bin: "long", games: 8900, win_rate_tenths: 470 },
          ],
        },
      };
      break;
    case "unsupported_output":
      view.overlay = { visible: false, hidden_reason: "unsupported_output" };
      break;
    case "list_build_disabled":
      view.overlay = { visible: false, hidden_reason: "list_build_disabled" };
      break;
    case "bar_off":
      view.overlay = { visible: false, hidden_reason: "bar_off" };
      break;
    case "hidden":
      view.mode = "hidden";
      view.overlay = { visible: false, hidden_reason: "no_draft" };
      break;
  }
  if (!mockPrefs.draft_bar_enabled && view.overlay.visible) view.overlay = { visible: false, hidden_reason: "bar_off" };
  view.source.table_requests_24h = tableRequestsUsed;
  return view;
}

/** Adds synthetic draft fixtures to the existing dev-only Tauri invoke harness for ?mock=1. */
export function installDraftMockCommands() {
  if (new URLSearchParams(window.location.search).get("mock") !== "1") return;
  const target = window as unknown as Window & { __TAURI_INTERNALS__?: MockInternals };
  const internals = target.__TAURI_INTERNALS__;
  if (!internals) return;
  const originalInvoke = internals.invoke.bind(internals);
  internals.invoke = async (command, args = {}) => {
    if (command === "draft_column_view") {
      const scenario = new URLSearchParams(window.location.search).get("draft") ?? "live";
      return fixtureView(scenario);
    }
    if (command === "draft_stats_prefs_get") return { ...mockPrefs };
    if (command === "draft_stats_prefs_set") {
      const patch = args.patch;
      if (typeof patch !== "object" || patch === null || Array.isArray(patch)) throw new Error("draft_stats_prefs_set requires a patch");
      const candidate = patch as Record<string, unknown>;
      for (const key of Object.keys(candidate)) {
        if (!["hero_stats_consent", "draft_bar_enabled", "position", "bracket"].includes(key)) {
          throw new Error(`draft_stats_prefs_set rejects field ${key}`);
        }
      }
      for (const key of ["hero_stats_consent", "draft_bar_enabled"] as const) {
        if (Object.hasOwn(candidate, key)) {
          if (typeof candidate[key] !== "boolean") throw new Error(`invalid ${key}`);
          mockPrefs[key] = candidate[key] as boolean;
        }
      }
      if (Object.hasOwn(candidate, "position")) {
        const position = candidate.position;
        if (position !== null && (!Number.isInteger(position) || (position as number) < 1 || (position as number) > 5)) throw new Error("invalid position");
        mockPrefs.position = position as number | null;
      }
      if (Object.hasOwn(candidate, "bracket")) {
        const bracket = candidate.bracket;
        if (bracket !== null && (!Number.isInteger(bracket) || (bracket as number) < 1 || (bracket as number) > 8)) throw new Error("invalid bracket");
        mockPrefs.bracket = bracket as number | null;
      }
      return { ...mockPrefs };
    }
    if (command === "draft_stats_retry") {
      if (tableRequestsUsed >= 4) throw new Error("DRAFT_STATS_BUDGET_SPENT");
      tableRequestsUsed += 1;
      return { accepted: true };
    }
    return originalInvoke(command, args);
  };
}
