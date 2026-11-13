import assert from "node:assert/strict";
import { test } from "node:test";

const packs = await import("../src/packs.ts");
const probe = await import("../src/probe.ts");

const sha = "c".repeat(64);
const record = {
  install_id: "ins_0000000002",
  package_id: "demo.pack",
  revision: 3,
  container_sha256: sha,
  authority_class: "local_import",
  publisher_claim: "none",
};
const listing = {
  active: { install_id: "ins_0000000001", package_id: "demo.pack", revision: 2 },
  records: [{ ...record, install_id: "ins_0000000001", revision: 2 }, record],
  pending: ["ins_0000000002"],
};
const result = (over = {}) => ({
  schema_version: "1.0.0",
  request_id: "req_abcdef0123456789",
  status: "installed",
  reason: null,
  record,
  ...over,
});

test("parses a valid media_list result", () => {
  assert.deepEqual(packs.parseMediaListing(listing), listing);
  assert.deepEqual(packs.parseMediaListing({ active: null, records: [], pending: [] }), {
    active: null,
    records: [],
    pending: [],
  });
});

test("an unreadable media_list result is null, never a partial listing", () => {
  assert.equal(packs.parseMediaListing(null), null);
  assert.equal(packs.parseMediaListing({ ...listing, extra: 1 }), null);
  assert.equal(packs.parseMediaListing({ ...listing, active: { install_id: "x" } }), null);
  assert.equal(packs.parseMediaListing({ ...listing, records: [{ ...record, revision: 0 }] }), null);
  assert.equal(packs.parseMediaListing({ ...listing, records: [{ ...record, authority_class: "publisher" }] }), null);
  assert.equal(packs.parseMediaListing({ ...listing, records: [{ ...record, container_sha256: "abc" }] }), null);
  assert.equal(packs.parseMediaListing({ ...listing, pending: ["bad id"] }), null);
});

test("parses the inspect result and rejects extra or invalid fields", () => {
  assert.deepEqual(packs.parseInspectResult({ container_sha256: sha, size_bytes: 10 }), {
    container_sha256: sha,
    size_bytes: 10,
  });
  assert.equal(packs.parseInspectResult({ container_sha256: sha }), null);
  assert.equal(packs.parseInspectResult({ container_sha256: "A".repeat(64), size_bytes: 10 }), null);
  assert.equal(packs.parseInspectResult({ container_sha256: sha, size_bytes: -1 }), null);
});

test("parses install results and enforces the status/record/reason shape", () => {
  assert.equal(packs.parseInstallResult(result())?.status, "installed");
  assert.equal(packs.parseInstallResult(result({ status: "active" }))?.record?.install_id, "ins_0000000002");
  assert.equal(
    packs.parseInstallResult(result({ status: "deferred", reason: "ACTIVATION_DEFERRED" }))?.reason,
    "ACTIVATION_DEFERRED",
  );
  const rejected = packs.parseInstallResult(result({ status: "rejected", reason: "MEDIA_INVALID", record: null }));
  assert.equal(rejected?.status, "rejected");
  assert.equal(rejected?.record, null);

  assert.equal(packs.parseInstallResult(result({ status: "rejected", reason: null, record: null })), null);
  assert.equal(packs.parseInstallResult(result({ status: "rejected", reason: "MEDIA_INVALID" })), null);
  assert.equal(packs.parseInstallResult(result({ record: null })), null);
  assert.equal(packs.parseInstallResult(result({ status: "unknown" })), null);
  assert.equal(packs.parseInstallResult(result({ schema_version: "2.0.0" })), null);
  assert.equal(packs.parseInstallResult(result({ reason: "lowercase" })), null);
});

test("builds a closed install request that never activates", () => {
  const request = packs.buildInstallRequest({
    request_id: "req_abcdef0123456789",
    source_path: "C:\\packs\\demo.zip",
    expected_container_sha256: sha,
  });
  assert.deepEqual(request, {
    schema_version: "1.0.0",
    request_id: "req_abcdef0123456789",
    profile_id: packs.PACK_PROFILE_ID,
    source_path: "C:\\packs\\demo.zip",
    expected_container_sha256: sha,
    project_binding: null,
    activation_policy: "never",
  });
  assert.equal(packs.buildInstallRequest({ request_id: "short", source_path: "C:\\a.zip", expected_container_sha256: sha }), null);
  assert.equal(
    packs.buildInstallRequest({ request_id: "req_abcdef0123456789", source_path: "C:\\a.zip", expected_container_sha256: "x" }),
    null,
  );
  assert.equal(
    packs.buildInstallRequest({
      request_id: "req_abcdef0123456789",
      source_path: "a".repeat(1025),
      expected_container_sha256: sha,
    }),
    null,
  );
});

test("request ids satisfy the contract pattern", () => {
  for (let i = 0; i < 20; i += 1) assert.match(packs.newRequestId(), /^[A-Za-z0-9_-]{8,64}$/);
  assert.match(packs.newRequestId(() => "zz"), /^req_[0-9a-f]{16,32}$/);
});

test("normalizes pasted paths", () => {
  assert.equal(packs.normalizeSourcePath("  \"C:\\a b\\p.zip\"  "), "C:\\a b\\p.zip");
  assert.equal(packs.normalizeSourcePath("C:\\p.zip"), "C:\\p.zip");
  assert.equal(packs.normalizeSourcePath("\""), "\"");
  assert.equal(packs.isPlausibleSourcePath("ab"), false);
  assert.equal(packs.isPlausibleSourcePath("C:\\"), true);
});

test("command arguments use camelCase keys that Tauri 2 maps to snake_case parameters", () => {
  assert.deepEqual(packs.inspectArgs("C:\\p.zip"), { sourcePath: "C:\\p.zip" });
  assert.deepEqual(packs.activateArgs("req_abcdef0123", "ins_0000000002", "after_match"), {
    requestId: "req_abcdef0123",
    installId: "ins_0000000002",
    policy: "after_match",
  });
  assert.deepEqual(
    probe.probeReportArgs({ install_id: "ins_0000000001", revision: 4, path_sha256: sha, ok: true, duration_ms: 900 }),
    { installId: "ins_0000000001", revision: 4, pathSha256: sha, ok: true, durationMs: 900 },
  );
});

test("host error codes are normalised", () => {
  assert.equal(packs.errorCode("MEDIA_INVALID"), "MEDIA_INVALID");
  assert.equal(packs.errorCode(new Error("CAPABILITY_UNAVAILABLE")), "CAPABILITY_UNAVAILABLE");
  assert.equal(packs.errorCode("some free text"), "UNKNOWN");
  assert.equal(packs.errorCode(undefined), "UNKNOWN");
});

test("pack state comes from the listing, and records sort newest first", () => {
  const parsed = packs.parseMediaListing(listing);
  assert.ok(parsed);
  const [first, second] = parsed.records;
  assert.ok(first && second);
  assert.equal(packs.packState(first, parsed), "active");
  assert.equal(packs.packState(second, parsed), "pending");
  assert.equal(packs.packState(second, { ...parsed, pending: [] }), "installed");
  assert.deepEqual(packs.sortRecords(parsed.records).map((item) => item.install_id), [
    "ins_0000000002",
    "ins_0000000001",
  ]);
});

test("formats sizes", () => {
  assert.equal(packs.formatBytes(512), "512 B");
  assert.equal(packs.formatBytes(1536), "1.5 KiB");
  assert.equal(packs.formatBytes(1_572_864), "1.5 MiB");
});
