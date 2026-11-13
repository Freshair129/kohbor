// Announcer-pack store shapes and pure helpers for the media_* host commands.
//
// The host owns installation, verification and activation. This module only
// parses what the host returns and builds the logical requests it accepts
// (contracts/media-install-request 1.0.0 and media-install-result 1.0.0). The
// `media_list` result is a host-local shape, not a frozen contract, so it is
// parsed strictly and an unreadable listing is reported as unreadable rather
// than guessed.

/**
 * profile_id is required by the install request, but no profile registry exists
 * yet (MEDIA-PACK-SPEC names it without defining values). The contract fixtures
 * use "default"; this constant is that assumption, kept in one place.
 */
export const PACK_PROFILE_ID = "default";

export const MAX_SOURCE_PATH_CHARS = 1024;
const MIN_SOURCE_PATH_CHARS = 3;

/** Policies the user may choose when activating an installed revision. `never` is install-only. */
export type ActivationPolicy = "never" | "now_if_idle" | "after_match";
export type ChosenActivationPolicy = Exclude<ActivationPolicy, "never">;

export type InstallStatus = "installed" | "active" | "deferred" | "activation_error" | "rejected";

export interface PackRecord {
  install_id: string;
  package_id: string;
  revision: number;
  container_sha256: string;
  authority_class: "local_import";
  publisher_claim: "none" | "unverified";
}

export interface ActivePack {
  install_id: string;
  package_id: string;
  revision: number;
}

export interface MediaListing {
  active: ActivePack | null;
  records: PackRecord[];
  /** Install ids whose activation is waiting for the match barrier or a playback lease. */
  pending: string[];
}

export interface InspectResult {
  container_sha256: string;
  size_bytes: number;
}

export interface InstallResult {
  request_id: string;
  status: InstallStatus;
  reason: string | null;
  record: PackRecord | null;
}

export type PackState = "active" | "pending" | "installed";

const INSTALL_ID = /^[A-Za-z0-9_-]{8,64}$/;
const PACKAGE_ID = /^[a-z0-9][a-z0-9_.-]{0,79}$/;
const SHA256 = /^[a-f0-9]{64}$/;
const REQUEST_ID = /^[A-Za-z0-9_-]{8,64}$/;
const REASON_CODE = /^[A-Z][A-Z0-9_]{1,63}$/;
const STATUSES: readonly InstallStatus[] = ["installed", "active", "deferred", "activation_error", "rejected"];

function objectRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(value);
  return keys.length === expected.length && expected.every((key) => Object.hasOwn(value, key));
}

function safeInteger(value: unknown, minimum: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= minimum;
}

function matches(pattern: RegExp, value: unknown): value is string {
  if (typeof value !== "string") return false;
  const match = pattern.exec(value);
  return match !== null && match[0] === value;
}

function parseRecord(value: unknown): PackRecord | null {
  const record = objectRecord(value);
  if (!record || !hasExactKeys(record, [
    "install_id", "package_id", "revision", "container_sha256", "authority_class", "publisher_claim",
  ])) return null;
  if (!matches(INSTALL_ID, record.install_id) || !matches(PACKAGE_ID, record.package_id)) return null;
  if (!safeInteger(record.revision, 1) || !matches(SHA256, record.container_sha256)) return null;
  if (record.authority_class !== "local_import") return null;
  if (record.publisher_claim !== "none" && record.publisher_claim !== "unverified") return null;
  return {
    install_id: record.install_id,
    package_id: record.package_id,
    revision: record.revision,
    container_sha256: record.container_sha256,
    authority_class: record.authority_class,
    publisher_claim: record.publisher_claim,
  };
}

function parseActive(value: unknown): ActivePack | null | undefined {
  if (value === null) return null;
  const active = objectRecord(value);
  if (!active || !hasExactKeys(active, ["install_id", "package_id", "revision"])) return undefined;
  if (!matches(INSTALL_ID, active.install_id) || !matches(PACKAGE_ID, active.package_id)) return undefined;
  if (!safeInteger(active.revision, 1)) return undefined;
  return { install_id: active.install_id, package_id: active.package_id, revision: active.revision };
}

/** Parse the `media_list` result. Any malformed part makes the whole listing unreadable. */
export function parseMediaListing(value: unknown): MediaListing | null {
  const listing = objectRecord(value);
  if (!listing || !hasExactKeys(listing, ["active", "records", "pending"])) return null;
  const active = parseActive(listing.active);
  if (active === undefined) return null;
  if (!Array.isArray(listing.records) || !Array.isArray(listing.pending)) return null;

  const records: PackRecord[] = [];
  for (const raw of listing.records) {
    const record = parseRecord(raw);
    if (!record) return null;
    records.push(record);
  }
  const pending: string[] = [];
  for (const raw of listing.pending) {
    if (!matches(INSTALL_ID, raw)) return null;
    pending.push(raw);
  }
  return { active, records, pending };
}

/** Parse the `media_inspect` result (hash and size of the chosen file; nothing is installed). */
export function parseInspectResult(value: unknown): InspectResult | null {
  const result = objectRecord(value);
  if (!result || !hasExactKeys(result, ["container_sha256", "size_bytes"])) return null;
  if (!matches(SHA256, result.container_sha256) || !safeInteger(result.size_bytes, 0)) return null;
  return { container_sha256: result.container_sha256, size_bytes: result.size_bytes };
}

/** Parse a media-install-result 1.0.0 payload returned by `media_install` or `media_activate`. */
export function parseInstallResult(value: unknown): InstallResult | null {
  const result = objectRecord(value);
  if (!result || !hasExactKeys(result, ["schema_version", "request_id", "status", "reason", "record"])) return null;
  if (result.schema_version !== "1.0.0" || !matches(REQUEST_ID, result.request_id)) return null;
  if (typeof result.status !== "string" || !STATUSES.includes(result.status as InstallStatus)) return null;
  if (result.reason !== null && !matches(REASON_CODE, result.reason)) return null;
  const record = result.record === null ? null : parseRecord(result.record);
  if (result.record !== null && record === null) return null;
  // Contract: a rejection carries a reason and no record; every other status carries a record.
  if (result.status === "rejected" && (record !== null || result.reason === null)) return null;
  if (result.status !== "rejected" && record === null) return null;
  return {
    request_id: result.request_id,
    status: result.status as InstallStatus,
    reason: result.reason,
    record,
  };
}

/** Explorer's "Copy as path" wraps the path in double quotes; remove one matching pair and trim. */
export function normalizeSourcePath(input: string): string {
  const trimmed = input.trim();
  if (trimmed.length >= 2 && trimmed.startsWith("\"") && trimmed.endsWith("\"")) {
    return trimmed.slice(1, -1).trim();
  }
  return trimmed;
}

export function isPlausibleSourcePath(path: string): boolean {
  const length = [...path].length;
  return length >= MIN_SOURCE_PATH_CHARS && length <= MAX_SOURCE_PATH_CHARS && !path.includes("\0");
}

/** A request id that satisfies ^[A-Za-z0-9_-]{8,64}$. The random source is injectable for tests. */
export function newRequestId(randomHex: () => string = defaultRandomHex): string {
  const body = randomHex().toLowerCase().replace(/[^a-f0-9]/g, "");
  return `req_${body.padEnd(16, "0").slice(0, 32)}`;
}

function defaultRandomHex(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export interface InstallRequestInput {
  request_id: string;
  source_path: string;
  expected_container_sha256: string;
}

/**
 * Build the media-install-request 1.0.0 payload. Install never activates
 * (activation_policy "never"); activation is a separate, explicit command.
 */
export function buildInstallRequest(input: InstallRequestInput): Record<string, unknown> | null {
  if (!matches(REQUEST_ID, input.request_id)) return null;
  if (!isPlausibleSourcePath(input.source_path)) return null;
  if (!matches(SHA256, input.expected_container_sha256)) return null;
  return {
    schema_version: "1.0.0",
    request_id: input.request_id,
    profile_id: PACK_PROFILE_ID,
    source_path: input.source_path,
    expected_container_sha256: input.expected_container_sha256,
    project_binding: null,
    activation_policy: "never",
  };
}

/**
 * Tauri 2 maps JavaScript argument names in camelCase onto the snake_case Rust
 * parameters of a command unless the command opts out with
 * `rename_all = "snake_case"`. None of the media_* commands does, so these
 * builders emit camelCase keys. `media_install` takes its payload as one
 * `request` value whose inner keys stay snake_case (a JSON contract).
 */
export function inspectArgs(sourcePath: string): { sourcePath: string } {
  return { sourcePath };
}

export function activateArgs(
  requestId: string,
  installId: string,
  policy: ChosenActivationPolicy,
): { requestId: string; installId: string; policy: ChosenActivationPolicy } {
  return { requestId, installId, policy };
}

/** Host commands reject with the Err(String) code (for example "MEDIA_INVALID"). */
export function errorCode(error: unknown): string {
  const text = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return matches(REASON_CODE, text) ? text : "UNKNOWN";
}

export function packState(record: PackRecord, listing: MediaListing): PackState {
  if (listing.active?.install_id === record.install_id) return "active";
  return listing.pending.includes(record.install_id) ? "pending" : "installed";
}

/** Newest revision first; install ids are zero-padded counters, so a string compare is a numeric one. */
export function sortRecords(records: readonly PackRecord[]): PackRecord[] {
  return [...records].sort((a, b) => b.install_id.localeCompare(a.install_id));
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}
