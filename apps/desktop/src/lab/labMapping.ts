// Pure model of the Lab event-to-clip pairing (docs/design/KOHBOR-LAB.md section 12): the
// ACTIVE pack's bindings as the host reports them, and a dev mapping draft in the format the
// pack rebuild tool reads. The host validates the draft; the checks here only keep the form
// from offering impossible choices.

export interface PackBinding {
  eventId: string;
  tier: number | null;
  audioName: string;
  durationMs: number;
  hasVisual: boolean;
  state: "ok" | "invalid";
}

export interface ActiveBindings {
  active: { installId: string; packageId: string; revision: number } | null;
  bindings: PackBinding[];
}

export interface DraftBinding {
  eventId: string;
  tier: number | null;
  audio: string;
}

export interface MappableEvent {
  id: string;
  tierRange: [number, number] | null;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseActiveBindings(value: unknown): ActiveBindings | null {
  if (!isObject(value) || !Array.isArray(value.bindings)) return null;
  let active: ActiveBindings["active"] = null;
  if (value.active !== null && value.active !== undefined) {
    const a = value.active;
    if (!isObject(a) || typeof a.install_id !== "string" || typeof a.package_id !== "string" || typeof a.revision !== "number") {
      return null;
    }
    active = { installId: a.install_id, packageId: a.package_id, revision: a.revision };
  }
  const bindings: PackBinding[] = [];
  for (const row of value.bindings) {
    if (!isObject(row)) return null;
    const { event_id, tier, audio_name, duration_ms, has_visual, state } = row;
    if (typeof event_id !== "string" || typeof audio_name !== "string" || typeof duration_ms !== "number") return null;
    if (tier !== null && tier !== undefined && typeof tier !== "number") return null;
    if (state !== "ok" && state !== "invalid") return null;
    bindings.push({
      eventId: event_id,
      tier: typeof tier === "number" ? tier : null,
      audioName: audio_name,
      durationMs: duration_ms,
      hasVisual: has_visual === true,
      state,
    });
  }
  return { active, bindings };
}

export function bindingsFor(eventId: string, all: PackBinding[]): PackBinding[] {
  return all.filter((b) => b.eventId === eventId).sort((a, b) => (a.tier ?? 0) - (b.tier ?? 0));
}

export function formatDuration(ms: number): string {
  return `${(ms / 1000).toFixed(1)} วิ`;
}

export function bindingLabel(binding: PackBinding): string {
  const tier = binding.tier === null ? "" : `tier ${binding.tier}: `;
  const invalid = binding.state === "invalid" ? " (ตรวจซ้ำไม่ผ่าน เล่นไม่ได้)" : "";
  return `${tier}${binding.audioName} · ${formatDuration(binding.durationMs)}${binding.hasVisual ? " · มีแบนเนอร์" : ""}${invalid}`;
}

/** Play goes through the demo trigger (the real audio path); say why when it cannot. */
export function playAvailability(binding: PackBinding, demoOn: boolean): { ok: true } | { ok: false; reason: string } {
  if (binding.state !== "ok") return { ok: false, reason: "คลิปนี้ตรวจซ้ำไม่ผ่าน" };
  if (!demoOn) return { ok: false, reason: "ต้องเปิดแอปด้วย KOHBOR_DEMO=1" };
  return { ok: true };
}

/** Distinct clip names of the active pack, in first-seen order: the choices for a draft. */
export function clipChoices(all: PackBinding[]): string[] {
  return [...new Set(all.filter((b) => b.state === "ok").map((b) => b.audioName))];
}

export function draftKey(eventId: string, tier: number | null): string {
  return `${eventId}#${tier ?? "-"}`;
}

export type DraftProblem = "unknown_event" | "tier_required" | "tier_not_allowed" | "tier_out_of_range" | "bad_audio" | "duplicate";

const AUDIO_NAME = /^[A-Za-z0-9][A-Za-z0-9._-]{0,75}\.wav$/;

/** Why an entry cannot be added, or `null` when it can. The host re-validates everything. */
export function draftProblem(entry: DraftBinding, events: MappableEvent[], draft: DraftBinding[]): DraftProblem | null {
  const event = events.find((e) => e.id === entry.eventId);
  if (!event) return "unknown_event";
  if (event.tierRange === null && entry.tier !== null) return "tier_not_allowed";
  if (event.tierRange !== null) {
    if (entry.tier === null) return "tier_required";
    if (entry.tier < event.tierRange[0] || entry.tier > event.tierRange[1]) return "tier_out_of_range";
  }
  if (!AUDIO_NAME.test(entry.audio) || entry.audio.includes("..")) return "bad_audio";
  if (draft.some((d) => draftKey(d.eventId, d.tier) === draftKey(entry.eventId, entry.tier))) return "duplicate";
  return null;
}

export const DRAFT_PROBLEM_TH: Record<DraftProblem, string> = {
  unknown_event: "ไม่รู้จัก event นี้",
  tier_required: "event นี้ต้องเลือก tier",
  tier_not_allowed: "event นี้ไม่มี tier",
  tier_out_of_range: "tier อยู่นอกช่วงของ event",
  bad_audio: "ชื่อคลิปไม่ถูกต้อง",
  duplicate: "event/tier นี้ผูกไว้แล้ว",
};

/** The wire shape of `lab_pack_mapping_apply` (rebuild-tool format). */
export function toMappingDocument(draft: DraftBinding[]): { schema_version: string; bindings: Record<string, unknown>[] } {
  return {
    schema_version: "1.0.0",
    bindings: draft.map((d) =>
      d.tier === null ? { event_id: d.eventId, audio: d.audio } : { event_id: d.eventId, tier: d.tier, audio: d.audio },
    ),
  };
}

export function parseDraft(value: unknown): DraftBinding[] | null {
  if (value === null || value === undefined) return [];
  if (!isObject(value) || !Array.isArray(value.bindings)) return null;
  const out: DraftBinding[] = [];
  for (const row of value.bindings) {
    if (!isObject(row) || typeof row.event_id !== "string" || typeof row.audio !== "string") return null;
    if (row.tier !== undefined && row.tier !== null && typeof row.tier !== "number") return null;
    out.push({ eventId: row.event_id, tier: typeof row.tier === "number" ? row.tier : null, audio: row.audio });
  }
  return out;
}
