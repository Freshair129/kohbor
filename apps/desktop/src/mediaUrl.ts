const LOGICAL_MEDIA_URL = /^kohbor-media:\/\/localhost\/v1\/(ins_[0-9]{10})\/([0-9a-f]{64})$/;

/** Convert a validated logical media URL to the URL shape used by the webview. */
export function toWebviewUrl(logical: string, platform: "windows" | "other"): string | null {
  const match = LOGICAL_MEDIA_URL.exec(logical);
  if (!match || match[0] !== logical) return null;

  if (platform === "windows") {
    return `http://kohbor-media.localhost/v1/${match[1]}/${match[2]}`;
  }
  return logical;
}
