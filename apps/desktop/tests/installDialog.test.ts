import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { installDialogCopy } from "../src/packs/InstallDialog.copy.ts";
import {
  droppedZipPath,
  isZipPath,
  parseCancelPendingActivationResult,
  parsePackageSummary,
  pointInsideRect,
  readPackageFile,
} from "../src/packs/InstallDialog.model.ts";

const digest = "a".repeat(64);
const validSummary = {
  schema_version: "1.0.0",
  status: "valid",
  package_id: "demo.pack",
  display_name: "Demo voice pack",
  revision: 4,
  event_count: 3,
  binding_count: 4,
  variant_count: 7,
  banner_count: 2,
  size_bytes: 8192,
  container_sha256: digest,
  signature_state: "unavailable",
  trust_state: "local_import",
  publisher_claim: "unverified",
  rejection_reason: null,
};

test("parses a valid package summary with its display, counts, trust and size", () => {
  assert.deepEqual(parsePackageSummary(validSummary), validSummary);
});

test("preserves rejected package reasons and refuses incomplete summaries", () => {
  const rejected = parsePackageSummary({
    ...validSummary,
    status: "rejected",
    signature_state: "not_assessed",
    trust_state: "not_assessed",
    publisher_claim: "not_assessed",
    rejection_reason: "digest_mismatch",
  });
  assert.equal(rejected?.rejection_reason, "digest_mismatch");
  assert.equal(installDialogCopy.th.rejectionReasons.digest_mismatch, "เนื้อหาไฟล์ไม่ตรงกับค่าแฮชในแพ็ก");
  assert.equal(installDialogCopy.en.rejectionReasons.not_a_zip, "This is not a supported ZIP package.");
  assert.equal(parsePackageSummary({ ...validSummary, rejection_reason: "not_a_zip" }), null);
  assert.equal(parsePackageSummary({ ...validSummary, extra: true }), null);
});

test("parses pending-activation cancellation without accepting malformed host replies", () => {
  assert.deepEqual(parseCancelPendingActivationResult({
    schema_version: "1.0.0",
    install_id: "ins_0000000042",
    cancelled: true,
  }), {
    schema_version: "1.0.0",
    install_id: "ins_0000000042",
    cancelled: true,
  });
  assert.equal(parseCancelPendingActivationResult({ schema_version: "1.0.0", install_id: "x", cancelled: true }), null);
});

test("native drag accepts ZIP paths only when the pointer is over the drop zone", () => {
  assert.equal(isZipPath("C:\\packs\\voice.ZIP"), true);
  assert.equal(isZipPath("C:\\packs\\voice.zip.exe"), false);
  assert.equal(droppedZipPath(["a.txt", "voice.ZIP", "other.zip"]), "voice.ZIP");
  assert.equal(droppedZipPath(["a.txt", "voice.zip.exe"]), undefined);
  const dropZone = { left: 10, right: 110, top: 20, bottom: 70 };
  assert.equal(pointInsideRect({ x: 100, y: 80 }, dropZone, 2), true);
  assert.equal(pointInsideRect({ x: 300, y: 80 }, dropZone, 2), false);
});

test("picker rejects oversized files before reading and accepts the exact host limit", async () => {
  let reads = 0;
  const arrayBuffer = async () => {
    reads += 1;
    return new ArrayBuffer(4);
  };
  await assert.rejects(readPackageFile({ size: 5, arrayBuffer }, 4), { message: "PACKAGE_TOO_LARGE" });
  await assert.rejects(readPackageFile({ size: 4, arrayBuffer }, undefined), { message: "CAPABILITY_UNAVAILABLE" });
  assert.equal(reads, 0);
  assert.equal((await readPackageFile({ size: 4, arrayBuffer }, 4)).byteLength, 4);
  assert.equal(reads, 1);
});

test("ZIP picker uses a file input and requires no plugin capability", () => {
  const capability = JSON.parse(readFileSync(new URL("../src-tauri/capabilities/default.json", import.meta.url), "utf8")) as {
    permissions: string[];
  };
  assert.deepEqual(capability.permissions, ["core:default"]);

  const component = readFileSync(new URL("../src/packs/InstallDialog.tsx", import.meta.url), "utf8");
  assert.match(component, /<input\s+type="file"\s+accept="\.zip,application\/zip"/);
  assert.match(component, /invoke<unknown>\("media_package_summary_bytes", await readPickerBytes/);
  assert.match(component, /invoke\("media_install_bytes", await readPickerBytes/);
  assert.match(component, /getCurrentWebview\(\)\.onDragDropEvent\(/);
  assert.doesNotMatch(component, /plugin:dialog/);
  const packageJson = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")) as {
    dependencies: Record<string, string>;
  };
  assert.equal(Object.hasOwn(packageJson.dependencies, "@tauri-apps/plugin-dialog"), false);
});
