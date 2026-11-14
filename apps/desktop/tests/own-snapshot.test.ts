import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { parseOwnSnapshot, OWN_STAT_FIELDS } from "../src/ownSnapshot.ts";

// Keep legacy coverage explicit while the host fixture advances additively to 2.2.
const fixture = () => {
  const snapshot = JSON.parse(readFileSync(new URL("../../../fixtures/own-fresh-at-boundary.json", import.meta.url), "utf8"));
  snapshot.schema_version = "2.1.0";
  delete snapshot.own.fields.items; delete snapshot.own.fields.abilities;
  return snapshot;
};

test("2.1 snapshots retain fresh real zeros and unknowns without changing the input", () => {
  const input = fixture();
  input.own.fields.kills = { value: 0, availability: "fresh" };
  const parsed = parseOwnSnapshot(input);
  assert.equal(parsed?.schema_version, "2.1.0");
  assert.deepEqual(parsed?.own?.fields.kills, { value: 0, availability: "fresh" });
  assert.deepEqual(parsed?.own?.fields.gold, { value: null, availability: "unknown" });
  assert.notEqual(parsed?.own?.fields, input.own.fields);
});

test("2.0 snapshots normalize absent counters to unknown rather than zero", () => {
  const input = fixture(); input.schema_version = "2.0.0";
  for (const name of OWN_STAT_FIELDS) delete input.own.fields[name];
  const parsed = parseOwnSnapshot(input);
  assert.ok(parsed);
  for (const name of OWN_STAT_FIELDS) assert.deepEqual(parsed.own?.fields[name], { value: null, availability: "unknown" });
  assert.equal(Object.hasOwn(input.own.fields, "gold"), false);
});

test("malformed versions, missing 2.1 counters and contradictory availability are rejected", () => {
  const mutations = [
    (s: any) => { s.schema_version = "9.0.0"; },
    (s: any) => { delete s.own.fields.gold; },
    (s: any) => { s.own.fields.kills = { value: 0, availability: "unknown" }; },
    (s: any) => { s.own.fields.gold = { value: null, availability: "fresh" }; },
    (s: any) => { s.own.fields.gpm = { value: -1, availability: "fresh" }; },
    (s: any) => { s.own.fields.xpm = { value: 1.5, availability: "fresh" }; },
    (s: any) => { s.source.age_ms = 2001; },
    (s: any) => { s.own.fields.hp.value = 9999; },
    (s: any) => { s.availability = "unknown"; },
  ];
  for (const mutate of mutations) { const input = fixture(); mutate(input); assert.equal(parseOwnSnapshot(input), null); }
});

test("a stale contract snapshot parses with its age and retained stale counters", () => {
  const input = fixture(); input.availability = "stale"; input.source.age_ms = 2001;
  for (const field of Object.values(input.own.fields) as any[]) if (field.availability === "fresh") field.availability = "stale";
  input.own.fields.gold = { value: 50, availability: "stale" };
  assert.deepEqual(parseOwnSnapshot(input)?.own?.fields.gold, { value: 50, availability: "stale" });
});
