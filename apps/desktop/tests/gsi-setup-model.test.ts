import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { gsiNeedsAttention, gsiPhase, gsiSetupFailure, gsiStateLabel, isConfigConflict } from "../src/shell/gsiSetupModel.ts";

const status = (state: string, reason = "NONE") => ({ intake: { state, reason, port: 3000, generation: 1, last_admitted_age_ms: null } }) as never;

test("GSI phase maps each host intake state; unknown/missing never reads as receiving", () => {
  assert.equal(gsiPhase(null), null);
  assert.equal(gsiPhase(status("not_configured")), "not_installed");
  assert.equal(gsiPhase(status("listening")), "waiting_restart");
  assert.equal(gsiPhase(status("receiving")), "receiving");
  assert.equal(gsiPhase(status("stale")), "stale");
  assert.equal(gsiPhase(status("starting")), "starting");
  assert.equal(gsiPhase(status("unavailable", "GSI_PORT_UNAVAILABLE")), "problem");
});

test("the setup card stays visible until data is actually receiving", () => {
  assert.equal(gsiNeedsAttention(null), false);
  for (const state of ["not_configured", "listening", "stale", "starting", "unavailable"]) assert.equal(gsiNeedsAttention(gsiPhase(status(state))), true, state);
  assert.equal(gsiNeedsAttention(gsiPhase(status("receiving"))), false);
});

test("config conflict is detected only for the unavailable/conflict reason", () => {
  assert.equal(isConfigConflict(status("unavailable", "GSI_CONFIG_CONFLICT")), true);
  assert.equal(isConfigConflict(status("unavailable", "GSI_PORT_UNAVAILABLE")), false);
  assert.equal(isConfigConflict(status("listening", "GSI_CONFIG_CONFLICT")), false);
});

test("state labels cover not installed / waiting for Dota restart / receiving in both languages", () => {
  assert.match(gsiStateLabel("not_installed", "th"), /ยังไม่ได้ติดตั้ง/);
  assert.match(gsiStateLabel("waiting_restart", "th"), /เปิด Dota 2 ใหม่/);
  assert.match(gsiStateLabel("receiving", "th"), /กำลังรับข้อมูล/);
  assert.match(gsiStateLabel("waiting_restart", "en"), /Restart Dota 2/);
});

test("live page mounts the card, the loadout hint has a setup button, Settings keeps the Guide", () => {
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /<GsiSetupCard /);
  assert.match(app, /<Guide /);
  const loadout = readFileSync(new URL("../src/deck/OwnLoadout.tsx", import.meta.url), "utf8");
  assert.match(loadout, /ตั้งค่า GSI \/ ติดตั้งไฟล์เชื่อมต่อ Dota 2/);
  const card = readFileSync(new URL("../src/shell/GsiSetupCard.tsx", import.meta.url), "utf8");
  assert.match(card, /run\(true\)/, "replace confirmation flow kept");
  assert.match(card, /gsiSetupFailure\(status, failed\)/);
});

test("the live card reports the concrete host or setup failure", () => {
  const missing = status("unavailable", "GSI_GAME_NOT_FOUND");
  assert.equal(gsiSetupFailure(missing, "GSI_GAME_NOT_FOUND"), "GSI_GAME_NOT_FOUND");
  assert.equal(gsiSetupFailure(missing, null), "GSI_GAME_NOT_FOUND");
  assert.equal(gsiSetupFailure(missing, "GSI_CONFIG_WRITE_FAILED"), "GSI_CONFIG_WRITE_FAILED");
  assert.equal(gsiSetupFailure(status("not_configured"), "GSI_GAME_NOT_FOUND"), "GSI_GAME_NOT_FOUND");
  assert.equal(gsiSetupFailure(status("listening"), null), null);
});

test("the live card uses its dedicated replacement guidance for host-reported conflicts", () => {
  const conflict = status("unavailable", "GSI_CONFIG_CONFLICT");
  assert.equal(gsiSetupFailure(conflict, null), null);
  assert.equal(gsiSetupFailure(conflict, "GSI_CONFIG_CONFLICT"), null);
  assert.equal(gsiSetupFailure(conflict, "GSI_CONFIG_WRITE_FAILED"), "GSI_CONFIG_WRITE_FAILED");
  assert.equal(gsiSetupFailure(status("not_configured"), "GSI_CONFIG_CONFLICT"), "GSI_CONFIG_CONFLICT");
});
