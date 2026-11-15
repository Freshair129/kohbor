import assert from "node:assert/strict";
import test from "node:test";
import {
  ALL,
  EMPTY_FILTER,
  distinct,
  filterRows,
  formatClock,
  gMaidenLabel,
  lastFiredLabel,
  parseCatalog,
  summarize,
  tunablesLabel,
} from "../src/lab/labCatalog.ts";

function row(id: string, over: Record<string, unknown> = {}) {
  return {
    id,
    label_th: `ไทย ${id}`,
    label_en: id,
    category: "ordinary",
    source: "gsi",
    source_label_th: "GSI (ข้อมูลเกม)",
    status: "detected_today",
    status_th: "ตรวจจับได้แล้ว",
    detector_ref: "native.gsi.x",
    milestone: "M2",
    tunables: 0,
    tunables_editable: 0,
    g_maiden: [],
    last_fired: null,
    ...over,
  };
}

const TABLE = {
  events: [
    row("hero_kill", { g_maiden: [{ id: "kill", tier: null, relation: "same" }] }),
    row("health_danger", {
      category: "critical",
      tunables: 5,
      tunables_editable: 5,
      g_maiden: [{ id: "danger", tier: null, relation: "approx" }],
      last_fired: { count: 2, tier: null, at_ms: 125_000 },
    }),
    row("enemy_missing", { source: "gsi_cv", category: "notice", tunables: 2 }),
    row("clip_requested", { status: "catalog_only", category: "recording" }),
    row("latency_probe", { status: "test_only", source: "test_only", category: "test" }),
  ],
  not_in_catalog: [
    { id: "rune_spawn", label_th: "รูนเกิด", status: "proposed_not_built", note_th: "ออกแบบเท่านั้น", verified: false, built: false },
  ],
  g_maiden_only: [{ id: "advice", label_th: "คำแนะนำ", note_th: "เฉพาะ G-Maiden" }],
  note_th: "หมายเหตุ",
};

test("a well-formed table parses; any malformed row makes the whole table unreadable", () => {
  const view = parseCatalog(TABLE);
  assert.ok(view);
  assert.equal(view.rows.length, 5);
  assert.equal(view.rows[1].lastFired?.atMs, 125_000);
  assert.equal(view.notInCatalog[0].status, "proposed_not_built");
  assert.equal(view.gMaidenOnly[0].id, "advice");
  assert.equal(parseCatalog({ ...TABLE, events: [{ id: "x" }] }), null);
  assert.equal(parseCatalog(null), null);
  const badStatus = { ...TABLE, not_in_catalog: [{ id: "a", label_th: "a", note_th: "a", status: "built" }] };
  assert.equal(parseCatalog(badStatus), null);
});

test("filters combine and match Thai, English and G-Maiden ids", () => {
  const view = parseCatalog(TABLE);
  assert.ok(view);
  assert.equal(filterRows(view.rows, EMPTY_FILTER).length, 5);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, category: "critical" }).map((r) => r.id), ["health_danger"]);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, source: "gsi_cv" }).map((r) => r.id), ["enemy_missing"]);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, status: "catalog_only" }).map((r) => r.id), ["clip_requested"]);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, text: "DANGER" }).map((r) => r.id), ["health_danger"]);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, text: "kill" }).map((r) => r.id), ["hero_kill"]);
  assert.deepEqual(filterRows(view.rows, { ...EMPTY_FILTER, text: "ไม่มีคำนี้" }).map((r) => r.id), []);
  assert.deepEqual(filterRows(view.rows, { category: "ordinary", source: ALL, status: "test_only", text: "" }), []);
});

test("distinct values keep first-seen order and summaries count by status", () => {
  const view = parseCatalog(TABLE);
  assert.ok(view);
  assert.deepEqual(distinct(view.rows, "status"), ["detected_today", "catalog_only", "test_only"]);
  assert.deepEqual(summarize(view.rows), { total: 5, detected: 3, catalogOnly: 1, testOnly: 1 });
});

test("session clock and last-fired text", () => {
  assert.equal(formatClock(0), "0:00");
  assert.equal(formatClock(125_000), "2:05");
  assert.equal(formatClock(3_725_000), "1:02:05");
  assert.equal(lastFiredLabel(null), "ยังไม่เคย");
  assert.equal(lastFiredLabel({ count: 2, tier: null, atMs: 125_000 }), "2 ครั้ง · ล่าสุด 2:05");
  assert.equal(lastFiredLabel({ count: 1, tier: 3, atMs: 5000 }), "1 ครั้ง · ล่าสุด 0:05 (tier 3)");
});

test("G-Maiden and tunable labels say what is approximate or not editable", () => {
  assert.equal(gMaidenLabel([]), "ไม่มี (เฉพาะของเรา)");
  assert.equal(gMaidenLabel([{ id: "kill", tier: null, relation: "same" }]), "kill");
  assert.equal(gMaidenLabel([{ id: "danger", tier: null, relation: "approx" }]), "~ danger");
  assert.equal(
    gMaidenLabel([
      { id: "double_kill", tier: 2, relation: "tier" },
      { id: "triple_kill", tier: 3, relation: "tier" },
    ]),
    "double_kill (tier 2), triple_kill (tier 3)",
  );
  const view = parseCatalog(TABLE);
  assert.ok(view);
  assert.equal(tunablesLabel(view.rows[0]), "—");
  assert.equal(tunablesLabel(view.rows[1]), "5/5 แก้ได้");
  assert.equal(tunablesLabel(view.rows[2]), "2 (ยังแก้ใน Lab ไม่ได้)");
});
