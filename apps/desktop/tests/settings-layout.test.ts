import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { copy } from "../src/copy.ts";
import { validateLeadTimeInputs } from "../src/objectivePrefs.ts";
import {
  SETTINGS_CONTROL_IDS,
  SETTINGS_SECTIONS,
} from "../src/settings/settingsModel.ts";

const modelPath = new URL("../src/settings/settingsModel.ts", import.meta.url);
const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const objectiveSettings = readFileSync(new URL("../src/ObjectiveSettings.tsx", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
const settingsPage = readFileSync(new URL("../src/settings/SettingsPage.tsx", import.meta.url), "utf8");
const analyticsSettings = readFileSync(new URL("../src/analytics/AnalyticsSettings.tsx", import.meta.url), "utf8");
const analyticsSettingsStyles = readFileSync(new URL("../src/analytics/analyticsSettings.css", import.meta.url), "utf8");
const draftSettingsStyles = readFileSync(new URL("../src/draft/draft-column.css", import.meta.url), "utf8");
const orbSettingsStyles = readFileSync(new URL("../src/orb/orbSettings.css", import.meta.url), "utf8");
const dialogStyles = readFileSync(new URL("../src/shared/dialog.css", import.meta.url), "utf8");

function declarations(css: string, selector: string): string[] {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/gs)]
    .filter((match) => match[1].split(",").some((entry) => entry.trim() === selector))
    .map((match) => match[2]);
}

test("automatic replay setting copy is present in Thai and English", () => {
  assert.equal(copy("th").shell.replayAutoParseLabel, "แยกรีเพลย์อัตโนมัติ");
  assert.match(copy("th").shell.replayAutoParseHelp, /หลังออกจากแมตช์/u);
  assert.equal(copy("en").shell.replayAutoParseLabel, "Parse replays automatically");
  assert.match(copy("en").shell.replayAutoParseHelp, /after you leave the match/i);
  assert.match(copy("en").shell.replayAutoParseHelp, /one at a time/i);
  assert.match(copy("en").shell.replayAutoParseHelp, /low priority/i);
  assert.equal((analyticsSettings.match(/checked=\{settings\.auto_analyse\}/g) ?? []).length, 1);
  assert.match(analyticsSettings, /text\.replayAutoParseLabel/);
  assert.match(analyticsSettings, /text\.replayAutoParseHelp/);
  assert.equal(SETTINGS_CONTROL_IDS.filter((id) => id === "replayAutoParse").length, 1);
  assert.ok(SETTINGS_SECTIONS.find((section) => section.id === "replay-ai")?.controlIds.includes("replayAutoParse"));
});

test("every existing settings control and renderer section occurs exactly once", async () => {
  assert.ok(existsSync(fileURLToPath(modelPath)), "settings section model is present");
  const model = await import("../src/settings/settingsModel.ts");
  const expectedControls = [
    "locale", "steamName", "glassLevel", "developerMode", "objectiveAlerts", "mapKeyDiagnostics",
    "objectiveCardLayout", "replayStorage", "replayAutoParse", "analyticsAssistant", "showAppHotkey",
    "minimapCropPosition", "minimapCropSize", "minimapCropReset", "minimapDetectCrop", "minimapCropProposal",
    "minimapCropConfirm", "captureDisplayRate", "capturePause", "mapFeedback", "captureRetry", "captureResourceDetails",
    "mockCaptureReason",
    "newsFeed", "ownHistoryOpenDota", "publicHeroStats", "draftBar", "draftPosition", "draftBracket",
    "orbEnable", "orbDwell", "orbFocus", "orbMixer", "orbMusicApp", "orbMixerFocus", "orbMixerReset",
    "gsiConnection",
  ];
  const expectedComponents = [
    "generalPreferences", "developerPreferences", "objectiveAlerts", "objectiveCardLayout", "minimapCapture",
    "gsiConnection", "replayAnalytics", "showAppHotkey", "newsFeed", "ownHistoryOpenDota", "publicHeroStats", "orbAudio",
  ];
  assert.deepEqual([...model.SETTINGS_CONTROL_IDS].sort(), [...expectedControls].sort());
  assert.deepEqual([...model.SETTINGS_COMPONENT_IDS].sort(), [...expectedComponents].sort());
  const controlOccurrences = model.SETTINGS_SECTIONS.flatMap((section) => section.controlIds);
  const componentOccurrences = model.SETTINGS_SECTIONS.flatMap((section) => section.componentIds);
  assert.deepEqual([...controlOccurrences].sort(), [...expectedControls].sort());
  assert.deepEqual([...componentOccurrences].sort(), [...expectedComponents].sort());
  for (const id of expectedControls) assert.equal(controlOccurrences.filter((value) => value === id).length, 1, id);
  for (const id of expectedComponents) assert.equal(componentOccurrences.filter((value) => value === id).length, 1, id);
  for (const tag of ["ObjectiveSettings", "LayoutEditor", "MinimapPanel", "Guide", "AnalyticsSettings", "ShowHotkeySettings", "DraftSettings", "NewsSettings", "OpenDotaSettings", "OrbSettings"]) {
    assert.equal((appSource.match(new RegExp(`<${tag}\\b`, "g")) ?? []).length, 1, tag);
  }
  for (const assignment of [
    '"overlay-layout": <LayoutEditor',
    '"minimap-capture": (',
    '"replay-ai": <AnalyticsSettings',
    'hotkeys: <ShowHotkeySettings',
    '"privacy-data": <><DraftSettings /><NewsSettings locale=',
    '<OpenDotaSettings locale=',
    '"audio-controls": <OrbSettings',
  ]) assert.ok(appSource.includes(assignment), assignment);
  assert.match(settingsPage, /SETTINGS_SECTIONS\.map/);
  assert.doesNotMatch(settingsPage, /ReplayAutoParseSetting/);
  assert.match(settingsPage, /initialSection\?: "overlay-layout" \| null/);
  assert.match(settingsPage, /hidden=\{activeSection !== section\.id\}/);
  assert.match(appSource, /<div className="shell-screen shell-settings">\s*<ObjectiveSettings/);
});

test("last section is remembered and a layout entry point selects Overlay layout", async () => {
  assert.ok(existsSync(fileURLToPath(modelPath)), "settings section model is present");
  const model = await import("../src/settings/settingsModel.ts");
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
  model.rememberSettingsSection("privacy-data", storage);
  assert.equal(model.readSettingsSection(storage), "privacy-data");
  assert.equal(model.initialSettingsSection("layout", "privacy-data"), "overlay-layout");
  assert.equal(model.initialSettingsSection(null, "privacy-data"), "privacy-data");
  assert.equal(model.initialSettingsSection(null, "invalid-section"), model.DEFAULT_SETTINGS_SECTION);
});

test("section persistence ignores unavailable or throwing localStorage", async () => {
  assert.ok(existsSync(fileURLToPath(modelPath)), "settings section model is present");
  const model = await import("../src/settings/settingsModel.ts");
  const unavailable = { getItem: () => null, setItem: () => undefined };
  const throwing = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } };
  assert.equal(model.readSettingsSection(unavailable), model.DEFAULT_SETTINGS_SECTION);
  assert.equal(model.readSettingsSection(throwing), model.DEFAULT_SETTINGS_SECTION);
  assert.doesNotThrow(() => model.rememberSettingsSection("general", throwing));
});

test("Settings has one content scroll area and a full-width 16:9 layout preview", () => {
  const content = declarations(styles, ".settings-content").join("\n");
  assert.match(content, /overflow-y:\s*auto/);
  assert.match(content, /overflow-x:\s*hidden/);
  assert.match(content, /min-width:\s*0/);
  const preview = declarations(styles, ".layout-editor-preview").join("\n");
  assert.match(preview, /width:\s*100%/);
  assert.match(preview, /aspect-ratio:\s*16\s*\/\s*9/);
  assert.match(preview, /max-width:\s*none/);
  const settingsScrollRules = [...styles.matchAll(/([^{}]+)\{([^{}]*)\}/gs)].filter((match) =>
    /\.shell-settings|\.settings-content|\.objective-settings|\.layout-editor|\.analytics-ledger-wrap/.test(match[1])
      && /overflow(?:-[xy])?\s*:\s*(?:auto|scroll)/.test(match[2]));
  assert.ok(settingsScrollRules.every((match) => match[1].includes(".settings-content")),
    settingsScrollRules.map((match) => match[1].trim()).join("\n"));
  assert.match(analyticsSettingsStyles, /\.analytics-ledger-wrap\s*\{[^}]*overflow:\s*auto/s);
  assert.match(styles, /\.settings-content\s+\.analytics-ledger-wrap\s*\{[^}]*overflow:\s*visible/s);
  assert.match(styles, /\.settings-content\s+\.draft-settings[^}]*overflow:\s*visible\s*!important/s);
  assert.match(orbSettingsStyles, /\.orb-settings\s*\{[^}]*overflow:\s*auto/s);
  assert.match(styles, /\.settings-content[^}]*\.orb-settings[^}]*overflow:\s*visible\s*!important/s);
  assert.match(dialogStyles, /\.dialog-body\s*\{[^}]*overflow:\s*auto/s);
  assert.match(styles, /\.settings-content\s+\.dialog-body\s*\{[^}]*overflow:\s*visible\s*!important/s);
  assert.match(draftSettingsStyles, /\.shell-screen\.shell-settings:has\(> \.draft-settings\)/);
  assert.match(styles, /\.settings-content :not\(\.layout-editor-preview \*\)\s*\{\s*font-size:\s*max\(12px,/s);
});

test("objective alert settings are one row per objective and retain lead-time validation", () => {
  assert.match(objectiveSettings, /<table/);
  assert.match(objectiveSettings, /OBJECTIVE_PREF_KEYS\.map/);
  assert.match(objectiveSettings, /validateLeadTimeInputs/);
  assert.match(objectiveSettings, /applyPatch\(\{ objectives: \{ \[id\]: \{ enabled \} \} \}, true\)/,
    "immediate enable changes preserve unsaved lead-time drafts");
  assert.match(objectiveSettings, /min=\{5\}\s+max=\{60\}/);
  assert.match(objectiveSettings, /noLeadTimes/);
  assert.equal((objectiveSettings.match(/copy\.saveLeadTimes/g) ?? []).length, 1);
  assert.match(objectiveSettings, /className="objective-settings-table"/);
});

test("objective row values keep the existing zero-to-two and 5-to-60 second validation", () => {
  assert.deepEqual(validateLeadTimeInputs(["", ""]), { values: [], error: null });
  assert.deepEqual(validateLeadTimeInputs(["5", "60"]), { values: [5, 60], error: null });
  assert.equal(validateLeadTimeInputs(["4", ""] ).error, "range");
  assert.equal(validateLeadTimeInputs(["", "61"]).error, "range");
  assert.equal(validateLeadTimeInputs(["30", "30"]).error, "duplicate");
});
