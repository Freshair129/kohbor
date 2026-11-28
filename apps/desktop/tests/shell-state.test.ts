import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { guestAccountPort } from "../src/account/accountView.ts";
import { reduceAccountMenu } from "../src/account/menuModel.ts";
import { accountFlyoutItems } from "../src/shell/accountFlyoutModel.ts";
import {
  DEFAULT_SHELL_DESTINATION,
  deriveHeaderState,
  nextShellDestination,
  navigationItemState,
  SHELL_NAV_ITEMS,
  statusLineModel,
} from "../src/shell/state.ts";

test("the two-rail navigation keeps the agreed destinations and one active item", () => {
  assert.deepEqual(SHELL_NAV_ITEMS.map((item) => item.id), [
    "overview", "live", "packs", "insights", "store", "status", "logs", "settings",
  ]);
  assert.equal(DEFAULT_SHELL_DESTINATION, "live");
  assert.deepEqual(navigationItemState("live", DEFAULT_SHELL_DESTINATION), { ariaCurrent: "page", tabIndex: 0 });
  assert.deepEqual(navigationItemState("overview", DEFAULT_SHELL_DESTINATION), { ariaCurrent: undefined, tabIndex: -1 });
  assert.deepEqual(navigationItemState("overview", "overview"), { ariaCurrent: "page", tabIndex: 0 });
  assert.deepEqual(navigationItemState("live", "account"), { ariaCurrent: undefined, tabIndex: 0 });
  assert.deepEqual(navigationItemState("overview", "account"), { ariaCurrent: undefined, tabIndex: -1 });
});

test("vertical rail keyboard navigation wraps and supports Home and End", () => {
  assert.equal(nextShellDestination("overview", "ArrowDown"), "live");
  assert.equal(nextShellDestination("overview", "ArrowUp"), "settings");
  assert.equal(nextShellDestination("packs", "Home"), "overview");
  assert.equal(nextShellDestination("packs", "End"), "settings");
  assert.equal(nextShellDestination("packs", "Enter"), null);
});

test("Analytics is a single destination without a Replay subview", () => {
  assert.equal(SHELL_NAV_ITEMS.some((item) => item.id === "replay"), false);
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.doesNotMatch(app, /insightsViews|ReplayPanel locale/);
});

test("status line maps live GSI, phase, age and capture values and routes setup to Status", () => {
  const model = statusLineModel({
    labels: {
      gsi: "GSI",
      gamePhase: "ระยะเกม",
      gameAge: "ข้อมูลเกม",
      capture: "จับภาพ",
      minimapRecording: "เปิดบันทึกภาพมินิแมป (Lab)",
      gsiSetup: "ตั้งค่า GSI",
    },
    gsi: { label: "กำลังรับข้อมูล", tone: "fresh" },
    phase: { label: "กำลังเล่น", tone: "fresh" },
    gameAge: "1.2 วิ",
    capture: { label: "ภาพสด · ทำงาน", tone: "fresh" },
    minimapRecording: true,
    deck: { label: "แสดงสถานะ", tone: "fresh", reason: "ข้อมูลจากสถานะปัจจุบัน" },
    setupAvailable: true,
    hostLost: false,
  });

  assert.deepEqual(model.items, [
    { id: "gsi", label: "GSI · กำลังรับข้อมูล", tone: "fresh" },
    { id: "phase", label: "ระยะเกม: กำลังเล่น", tone: "fresh" },
    { id: "age", label: "ข้อมูลเกม: 1.2 วิ", tone: "unknown" },
    { id: "capture", label: "จับภาพ · ภาพสด · ทำงาน", tone: "fresh" },
    { id: "recording", label: "เปิดบันทึกภาพมินิแมป (Lab)", tone: "warn" },
    { id: "deck", label: "แสดงสถานะ", tone: "fresh", detail: "ข้อมูลจากสถานะปัจจุบัน" },
  ]);
  assert.deepEqual(model.setupAction, { label: "ตั้งค่า GSI", destination: "status" });
  assert.equal(model.hostLost, false);
});

test("the account flyout contains available account actions without hidden social items", () => {
  const actions = accountFlyoutItems(guestAccountPort.get());
  assert.deepEqual(actions, ["account", "settings"]);
  assert.equal(actions.some((action) => ["friends", "notifications", "party", "add"].includes(action)), false);
  const opened = reduceAccountMenu({ open: false, index: 0 }, "toggle", actions.length).state;
  assert.deepEqual(opened, { open: true, index: 0 });
  assert.deepEqual(reduceAccountMenu(opened, "ArrowDown", actions.length).state, { open: true, index: 1 });
  assert.deepEqual(reduceAccountMenu(opened, "Escape", actions.length), { state: { open: false, index: 0 }, returnFocus: true });
});

test("header state remains explicitly unknown before GSI and capture data arrive", () => {
  assert.deepEqual(deriveHeaderState(null, null, false), {
    readiness: { label: "—", tone: "unknown" },
    gameAge: "—",
    capture: { label: "—", tone: "unknown" },
  });
});
