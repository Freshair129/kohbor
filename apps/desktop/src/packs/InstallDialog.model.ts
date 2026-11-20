export type InstallDialogLocale = "th" | "en";
export type PackageSummaryStatus = "valid" | "rejected";
export type SummaryTrustState = "local_import" | "not_assessed";
export type SummarySignatureState = "unavailable" | "not_assessed";
export type PublisherClaim = "none" | "unverified" | "not_assessed";

export interface PackageSummary {
  schema_version: "1.0.0";
  status: PackageSummaryStatus;
  package_id: string | null;
  display_name: string | null;
  revision: number | null;
  event_count: number | null;
  binding_count: number | null;
  variant_count: number | null;
  banner_count: number | null;
  size_bytes: number | null;
  container_sha256: string | null;
  signature_state: SummarySignatureState;
  trust_state: SummaryTrustState;
  publisher_claim: PublisherClaim;
  rejection_reason: string | null;
}

export interface CancelPendingActivationResult {
  schema_version: "1.0.0";
  install_id: string;
  cancelled: boolean;
}

const SHA256 = /^[a-f0-9]{64}$/;
const PACKAGE_ID = /^[a-z0-9][a-z0-9_.-]{0,79}$/;
const INSTALL_ID = /^ins_[0-9]{10}$/;

function objectRecord(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value);
  return actual.length === keys.length && keys.every((key) => Object.hasOwn(value, key));
}

function nullableText(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function nullableCount(value: unknown): value is number | null {
  return value === null || (typeof value === "number" && Number.isSafeInteger(value) && value >= 0);
}

/** Parse the strict host summary; malformed replies never produce a partial preview. */
export function parsePackageSummary(value: unknown): PackageSummary | null {
  const summary = objectRecord(value);
  if (!summary || !exactKeys(summary, [
    "schema_version", "status", "package_id", "display_name", "revision", "event_count",
    "binding_count", "variant_count", "banner_count", "size_bytes", "container_sha256",
    "signature_state", "trust_state", "publisher_claim", "rejection_reason",
  ])) return null;

  if (summary.schema_version !== "1.0.0" || (summary.status !== "valid" && summary.status !== "rejected")) return null;
  if (summary.package_id !== null && (typeof summary.package_id !== "string" || !PACKAGE_ID.test(summary.package_id))) return null;
  if (!nullableText(summary.display_name)) return null;
  if (summary.revision !== null && (typeof summary.revision !== "number" || !Number.isSafeInteger(summary.revision) || summary.revision < 1)) return null;
  if (![summary.event_count, summary.binding_count, summary.variant_count, summary.banner_count, summary.size_bytes].every(nullableCount)) return null;
  if (summary.container_sha256 !== null && (typeof summary.container_sha256 !== "string" || !SHA256.test(summary.container_sha256))) return null;
  if (summary.signature_state !== "unavailable" && summary.signature_state !== "not_assessed") return null;
  if (summary.trust_state !== "local_import" && summary.trust_state !== "not_assessed") return null;
  if (summary.publisher_claim !== "none" && summary.publisher_claim !== "unverified" && summary.publisher_claim !== "not_assessed") return null;
  if (!nullableText(summary.rejection_reason)) return null;

  if (summary.status === "valid") {
    if (summary.package_id === null || summary.display_name === null || summary.revision === null
      || summary.event_count === null || summary.binding_count === null || summary.variant_count === null
      || summary.banner_count === null || summary.size_bytes === null || summary.container_sha256 === null
      || summary.signature_state !== "unavailable" || summary.trust_state !== "local_import"
      || summary.publisher_claim === "not_assessed" || summary.rejection_reason !== null) return null;
  } else if (summary.rejection_reason === null) {
    return null;
  }

  return summary as unknown as PackageSummary;
}

export function parseCancelPendingActivationResult(value: unknown): CancelPendingActivationResult | null {
  const result = objectRecord(value);
  if (!result || !exactKeys(result, ["schema_version", "install_id", "cancelled"])) return null;
  if (result.schema_version !== "1.0.0" || typeof result.install_id !== "string" || !INSTALL_ID.test(result.install_id)
    || typeof result.cancelled !== "boolean") return null;
  return result as unknown as CancelPendingActivationResult;
}

export function isZipPath(path: string): boolean {
  return path.trim().toLowerCase().endsWith(".zip");
}

export function droppedZipPath(paths: readonly string[]): string | undefined {
  return paths.find(isZipPath);
}

/** Check the host's pack limit before allocating the file buffer. */
export async function readPackageFile(
  file: Pick<File, "size" | "arrayBuffer">,
  maxBytes: unknown,
): Promise<ArrayBuffer> {
  if (typeof maxBytes !== "number" || !Number.isSafeInteger(maxBytes) || maxBytes <= 0) {
    throw new Error("CAPABILITY_UNAVAILABLE");
  }
  if (file.size > maxBytes) throw new Error("PACKAGE_TOO_LARGE");
  return file.arrayBuffer();
}

export interface Point { x: number; y: number }
export interface Rect { left: number; right: number; top: number; bottom: number }

/** Tauri reports native drag positions in physical pixels; DOM rectangles use CSS pixels. */
export function pointInsideRect(point: Point, rect: Rect, devicePixelRatio: number): boolean {
  const scale = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  const x = point.x / scale;
  const y = point.y / scale;
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

export function formatPackageBytes(bytes: number, locale: InstallDialogLocale): string {
  if (bytes < 1024) return `${bytes} B`;
  const unit = bytes < 1024 * 1024 ? "KiB" : "MiB";
  const divisor = unit === "KiB" ? 1024 : 1024 * 1024;
  const number = new Intl.NumberFormat(locale === "th" ? "th-TH" : "en-US", {
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(bytes / divisor);
  return `${number} ${unit}`;
}

export function installResultMessage(
  status: "installed" | "active" | "deferred" | "activation_error" | "rejected",
  locale: InstallDialogLocale,
): string {
  const messages = locale === "th"
    ? {
      installed: "ติดตั้งแพ็กแล้ว",
      active: "ติดตั้งและเปิดใช้แพ็กแล้ว",
      deferred: "ติดตั้งแล้ว ระบบจะเปิดใช้หลังจบแมตช์",
      activation_error: "ติดตั้งแพ็กแล้ว แต่เปิดใช้ไม่สำเร็จ",
      rejected: "แพ็กนี้ติดตั้งไม่ได้",
    }
    : {
      installed: "Pack installed.",
      active: "Pack installed and activated.",
      deferred: "Pack installed. It will activate after the match.",
      activation_error: "Pack installed, but activation failed.",
      rejected: "This pack could not be installed.",
    };
  return messages[status];
}
