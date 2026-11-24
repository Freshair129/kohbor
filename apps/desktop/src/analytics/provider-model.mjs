const ANTHROPIC_MODELS = ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"];

export function visibleProviderRows(rows, showUntested) {
  return rows.filter((row) => row.verified === true || showUntested === true);
}

export function isLoopbackEndpoint(row) {
  if (row?.loopback !== true || typeof row.endpoint !== "string") return false;
  try {
    const url = new URL(row.endpoint);
    return ["http:", "https:"].includes(url.protocol)
      && ["127.0.0.1", "[::1]", "::1", "localhost"].includes(url.hostname.toLowerCase());
  } catch {
    return false;
  }
}

export function endpointClassLabel(row, copy) {
  return isLoopbackEndpoint(row) ? copy : "";
}

export function shouldWarnForEndpointChange(currentEndpoint, nextEndpoint) {
  return String(currentEndpoint ?? "").trim() !== String(nextEndpoint ?? "").trim();
}

export function availableModels(providerId, known = [], fetched = []) {
  if (providerId === "anthropic") return [...ANTHROPIC_MODELS];
  return [...new Set([...known, ...fetched].filter((model) => typeof model === "string" && model.length > 0))];
}

export function canEnableAi({ model, consent, requiresKey, keySet }) {
  return Boolean(model && consent && (!requiresKey || keySet));
}

export function providerErrorKind(error) {
  if (typeof error !== "string") return "unknown";
  if (error.includes("match_active")) return "match_active";
  if (error.includes("consent_required")) return "consent_required";
  if (error.includes("key_required")) return "key_required";
  if (error.includes("endpoint_invalid")) return "endpoint_invalid";
  if (error.includes("redirect_refused")) return "redirect_refused";
  if (error.includes("local_program_unavailable")) return "local_program_unavailable";
  if (error.includes("model_list_unavailable")) return "model_list_unavailable";
  if (error.includes("key_invalid")) return "key_invalid";
  return "unknown";
}

export function shouldOfferSaveAsNote({ answer, savedNoteId, noteFallback }) {
  return Boolean(answer?.trim() && !savedNoteId && noteFallback);
}
