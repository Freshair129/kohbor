import assert from "node:assert/strict";
import { test } from "node:test";

const { toWebviewUrl } = await import("../src/mediaUrl.ts");
const token = "a".repeat(64);
const logical = `kohbor-media://localhost/v1/ins_0000000001/${token}`;

test("converts a valid logical media URL on Windows", () => {
  assert.equal(
    toWebviewUrl(logical, "windows"),
    `http://kohbor-media.localhost/v1/ins_0000000001/${token}`,
  );
});

test("keeps a valid logical media URL on other platforms", () => {
  assert.equal(toWebviewUrl(logical, "other"), logical);
});

test("rejects a wrong scheme", () => {
  assert.equal(toWebviewUrl(logical.replace("kohbor-media://", "https://"), "windows"), null);
});

test("rejects an install id outside the required pattern", () => {
  assert.equal(toWebviewUrl(logical.replace("ins_0000000001", "ins_000000001"), "windows"), null);
});

test("rejects a token with the wrong length", () => {
  assert.equal(toWebviewUrl(`kohbor-media://localhost/v1/ins_0000000001/${"a".repeat(63)}`, "windows"), null);
});

test("rejects a query string", () => {
  assert.equal(toWebviewUrl(`${logical}?download=1`, "windows"), null);
});

test("rejects path traversal", () => {
  assert.equal(toWebviewUrl(`kohbor-media://localhost/v1/ins_0000000001/../${token}`, "windows"), null);
});

test("rejects uppercase token hex", () => {
  assert.equal(toWebviewUrl(`kohbor-media://localhost/v1/ins_0000000001/${"A".repeat(64)}`, "windows"), null);
});
