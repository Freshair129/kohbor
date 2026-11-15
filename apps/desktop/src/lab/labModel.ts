// Pure model of the Kohbor Lab view (docs/design/KOHBOR-LAB.md section 5). No React, no
// host calls: parsing of host JSON and the small rules the view shows.

export type GuardStatus = "pass" | "fail" | "unknown";

export interface Guard {
  name: string;
  lhs: number | null;
  op: string;
  rhs: number | null;
  unit: string;
  pass: boolean | null;
}

export interface EventTrace {
  id: string;
  fired: boolean;
  note: string | null;
  guards: Guard[];
}

export interface Trace {
  atMs: number;
  status: string;
  events: EventTrace[];
}

export interface ParamView {
  value: number | number[];
  default: number | number[];
  min: number;
  max: number;
  unit: string;
  changed: boolean;
}

export interface EventDescription {
  critical: boolean;
  params: Record<string, ParamView>;
}

export interface Description {
  profileVersion: string;
  taxonomyVersion: string;
  source: "embedded" | "override";
  embeddedSha256: string;
  overrides: string[];
  overridePath: string | null;
  rejected: { code: string; detail: string } | null;
  events: Record<string, EventDescription>;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function numberOrNull(value: unknown): number | null | undefined {
  if (value === null) return null;
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function formatValue(value: number | null, unit: string): string {
  if (value === null) return "—";
  if (unit === "bool") return value === 1 ? "yes" : "no";
  const text = Number.isInteger(value) ? String(value) : value.toFixed(1);
  return unit === "" ? text : `${text} ${unit}`;
}

export function guardStatus(pass: boolean | null): GuardStatus {
  if (pass === true) return "pass";
  if (pass === false) return "fail";
  return "unknown";
}

function parseGuard(value: unknown): Guard | null {
  if (!isObject(value)) return null;
  const lhs = numberOrNull(value.lhs);
  const rhs = numberOrNull(value.rhs);
  const pass = value.pass;
  if (typeof value.name !== "string" || typeof value.op !== "string" || typeof value.unit !== "string") return null;
  if (lhs === undefined || rhs === undefined) return null;
  if (pass !== null && typeof pass !== "boolean") return null;
  return { name: value.name, lhs, op: value.op, rhs, unit: value.unit, pass };
}

export function parseTrace(value: unknown): Trace | null {
  if (!isObject(value) || typeof value.at_ms !== "number" || typeof value.status !== "string") return null;
  if (!Array.isArray(value.events)) return null;
  const events: EventTrace[] = [];
  for (const row of value.events) {
    if (!isObject(row) || typeof row.id !== "string" || typeof row.fired !== "boolean" || !Array.isArray(row.guards)) {
      return null;
    }
    const guards = row.guards.map(parseGuard);
    if (guards.some((guard) => guard === null)) return null;
    events.push({
      id: row.id,
      fired: row.fired,
      note: typeof row.note === "string" ? row.note : null,
      guards: guards as Guard[],
    });
  }
  return { atMs: value.at_ms, status: value.status, events };
}

/** Guards that stop the event from firing: failing or not evaluable, in display order. */
export function blockingGuards(row: EventTrace): Guard[] {
  return row.guards.filter((guard) => guard.pass !== true);
}

function parseParam(value: unknown): ParamView | null {
  if (!isObject(value)) return null;
  const { min, max, unit, changed } = value;
  if (typeof min !== "number" || typeof max !== "number" || typeof unit !== "string" || typeof changed !== "boolean") {
    return null;
  }
  const scalarOrList = (v: unknown): number | number[] | null => {
    if (typeof v === "number") return v;
    if (Array.isArray(v) && v.every((n) => typeof n === "number")) return v as number[];
    return null;
  };
  const current = scalarOrList(value.value);
  const base = scalarOrList(value.default);
  if (current === null || base === null) return null;
  return { value: current, default: base, min, max, unit, changed };
}

export function parseDescription(value: unknown): Description | null {
  if (!isObject(value) || !isObject(value.events)) return null;
  if (typeof value.profile_version !== "string" || typeof value.taxonomy_version !== "string") return null;
  if (value.source !== "embedded" && value.source !== "override") return null;
  if (typeof value.embedded_default_sha256 !== "string" || !Array.isArray(value.overrides)) return null;
  const events: Record<string, EventDescription> = {};
  for (const [id, entry] of Object.entries(value.events)) {
    if (!isObject(entry) || typeof entry.critical !== "boolean" || !isObject(entry.params)) return null;
    const params: Record<string, ParamView> = {};
    for (const [name, raw] of Object.entries(entry.params)) {
      const param = parseParam(raw);
      if (param === null) return null;
      params[name] = param;
    }
    events[id] = { critical: entry.critical, params };
  }
  const rejected = isObject(value.rejected) && typeof value.rejected.code === "string"
    ? { code: value.rejected.code, detail: String(value.rejected.detail ?? "") }
    : null;
  return {
    profileVersion: value.profile_version,
    taxonomyVersion: value.taxonomy_version,
    source: value.source,
    embeddedSha256: value.embedded_default_sha256,
    overrides: value.overrides.filter((item): item is string => typeof item === "string"),
    overridePath: typeof value.override_path === "string" ? value.override_path : null,
    rejected,
    events,
  };
}

export interface DraftEdit {
  event: string;
  name: string;
  value: number;
}

export function draftKey(event: string, name: string): string {
  return `${event}.${name}`;
}

/** Record a pending edit; setting a value back to the current one drops the draft. */
export function withDraft(
  drafts: Record<string, DraftEdit>,
  edit: DraftEdit,
  current: number,
): Record<string, DraftEdit> {
  const key = draftKey(edit.event, edit.name);
  const next = { ...drafts };
  if (edit.value === current) delete next[key];
  else next[key] = edit;
  return next;
}

export function editsFromDrafts(drafts: Record<string, DraftEdit>): DraftEdit[] {
  return Object.values(drafts);
}

export function anyChanged(event: EventDescription): boolean {
  return Object.values(event.params).some((param) => param.changed);
}

export interface SafetyRow {
  event: string;
  status: "default" | "overridden";
  changed: string[];
}

/** The always-visible safety strip: critical events, whether any parameter moved. */
export function safetyReport(description: Description): SafetyRow[] {
  return Object.entries(description.events)
    .filter(([, event]) => event.critical)
    .map(([event, entry]) => {
      const changed = Object.entries(entry.params)
        .filter(([, param]) => param.changed)
        .map(([name]) => name);
      return { event, status: changed.length > 0 ? ("overridden" as const) : ("default" as const), changed };
    });
}
