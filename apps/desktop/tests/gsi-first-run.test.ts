import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  gsiPhase,
  readGsiFirstRunAnswer,
  shouldShowGsiFirstRun,
  writeGsiFirstRunAnswer,
} from "../src/shell/gsiSetupModel.ts";
import { dialogKeyAction } from "../src/shared/dialogModel.ts";

const status = (state: string, reason = "NONE") => ({ intake: { state, reason, port: null, generation: null, last_admitted_age_ms: null } }) as never;

test("first-run prompt shows only for not_installed with no stored answer", () => {
  assert.equal(shouldShowGsiFirstRun(gsiPhase(status("not_configured")), null, false), true);
  for (const phase of ["receiving", "waiting_restart", "stale", "problem", "starting", null] as const) {
    assert.equal(shouldShowGsiFirstRun(phase, null, false), false, String(phase));
  }
  for (const answer of ["accepted", "declined"] as const) {
    assert.equal(shouldShowGsiFirstRun("not_installed", answer, false), false, answer);
  }
  assert.equal(shouldShowGsiFirstRun("not_installed", null, true), false, "one offer per renderer run");
  assert.equal(gsiPhase(status("unavailable", "GSI_CONFIG_CONFLICT")), "problem");
  assert.equal(shouldShowGsiFirstRun(gsiPhase(status("unavailable", "GSI_CONFIG_CONFLICT")), null, false), false);
  assert.equal(shouldShowGsiFirstRun(gsiPhase(status("unavailable", "GSI_PORT_UNAVAILABLE")), null, false), false);
});

test("local storage errors leave the answer unset and do not escape", () => {
  assert.equal(readGsiFirstRunAnswer(() => { throw new Error("storage denied"); }), null);
  assert.equal(readGsiFirstRunAnswer(() => "unknown"), null);
  assert.equal(writeGsiFirstRunAnswer(() => { throw new Error("storage denied"); }), false);
  let stored = "";
  assert.equal(writeGsiFirstRunAnswer(() => { stored = "declined"; }), true);
  assert.equal(stored, "declined");
});

test("dialog uses the accessible shared modal and Escape reaches the intro decline handler", () => {
  const dialog = readFileSync(new URL("../src/shared/Dialog.tsx", import.meta.url), "utf8");
  const component = readFileSync(new URL("../src/shell/GsiFirstRunDialog.tsx", import.meta.url), "utf8");
  assert.match(dialog, /role="dialog" aria-modal="true"/);
  assert.equal(dialogKeyAction("Escape", false), "close");
  assert.match(component, /<Dialog[\s\S]*onClose=\{close\}/);
  assert.match(component, /if \(step === "intro"\) onDecline\(\)/);
});

test("connect action calls gsiSetup(false) and the dialog never offers replacement", () => {
  const component = readFileSync(new URL("../src/shell/GsiFirstRunDialog.tsx", import.meta.url), "utf8");
  assert.match(component, /setup\(false\)/);
  assert.doesNotMatch(component, /setup\(true\)/);
  assert.match(component, /result === "GSI_CONFIG_CONFLICT"[\s\S]*setStep\("conflict"\)/);
  assert.match(component, /if \(step === "intro"\) onDecline\(\)/);
});

test("mock harness exposes not_configured GSI without invoking native setup", () => {
  const harness = readFileSync(new URL("../src/mock/harness.tsx", import.meta.url), "utf8");
  assert.match(harness, /get\("mockGsi"\) === "not_configured"/);
  assert.match(harness, /status\.intake\.state = "not_configured"/);
  assert.match(harness, /if \(command === "gsi_setup"\) return null/);
});
