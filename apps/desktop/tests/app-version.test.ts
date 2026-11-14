import assert from "node:assert/strict";
import { test } from "node:test";
import { deriveAppVersionView, parseAppVersion, readAppVersion } from "../src/shell/appVersionModel.ts";

test("unknown app version stays dashed with no inferred profile or DEMO badge", () => {
  assert.deepEqual(deriveAppVersionView(null), { version: "—", compact: "—", commit: "—", buildDate: "—", profile: "—", demoBadge: null });
});

test("real app version projects compact version, first nine commit characters and DEMO", () => {
  const parsed = parseAppVersion({ version: "0.2.0-mvp2.1", git_commit: "123456789abcdef01234", build_date: "2026-11-14", profile: "release", demo: true });
  assert.deepEqual(deriveAppVersionView(parsed), { version: "0.2.0-mvp2.1", compact: "v0.2.0-mvp2.1", commit: "123456789", buildDate: "2026-11-14", profile: "release", demoBadge: "DEMO" });
  assert.equal(deriveAppVersionView(parseAppVersion({ version: "v1.2.3", profile: "debug", demo: false })).compact, "v1.2.3");
});

test("missing or invalid version fields remain individually unknown", () => {
  const view = deriveAppVersionView(parseAppVersion({ version: "1.2.3", git_commit: null, build_date: null, profile: "future", demo: "true" }));
  assert.equal(view.compact, "v1.2.3"); assert.equal(view.commit, "—"); assert.equal(view.buildDate, "—");
  assert.equal(view.profile, "—"); assert.equal(view.demoBadge, null);
  assert.equal(deriveAppVersionView(parseAppVersion({})).compact, "—");
  assert.equal(parseAppVersion(null), null);
});

test("older hosts without app_version never acquire a made-up version", async () => {
  const result = await readAppVersion(async () => { throw new Error("unknown command app_version"); });
  assert.equal(result, null);
  assert.equal(deriveAppVersionView(result).compact, "—");
});
