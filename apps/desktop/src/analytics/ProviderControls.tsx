import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AI_PROVIDER_COPY, type Locale } from "../copy";
import { readDeveloperMode } from "../shell/developerMode";
import {
  availableModels,
  canEnableAi,
  endpointClassLabel,
  providerErrorKind,
  shouldWarnForEndpointChange,
  visibleProviderRows,
} from "./provider-model.mjs";
import type { ProviderRow } from "./provider-model.mjs";
import "./ProviderControls.css";

export type ProviderPreferences = {
  ai_on: boolean;
  provider: string;
  consent: boolean;
  model: string;
  provider_endpoints: Record<string, string>;
  provider_consents: Record<string, { origin: string; text_version: number; granted_at_unix_ms: number }>;
  verified_providers: Record<string, string>;
  tools_modes: Record<string, string>;
  show_untested: boolean;
  keep_newest: number;
  max_bytes: number;
  auto_analyse: boolean;
};

type KeyStatus = { provider: string; set: boolean; last4: string | null; requires_key?: boolean };
type ProviderList = { providers: ProviderRow[]; untested_count: number; show_untested: boolean };

type Props = {
  locale: Locale;
  developerMode?: boolean;
  settings: ProviderPreferences;
  onSettingsChange: (settings: ProviderPreferences) => void;
  onKeyStatusChange?: (status: KeyStatus) => void;
  compact?: boolean;
  consentOpen?: boolean;
  onConsentOpenChange?: (open: boolean) => void;
};

function format(template: string, values: Record<string, string>) {
  return Object.entries(values).reduce((result, [key, value]) => result.replaceAll(`{${key}}`, value), template);
}

function friendlyError(reason: unknown, copy: (typeof AI_PROVIDER_COPY)[Locale]) {
  switch (providerErrorKind(reason)) {
    case "match_active": return copy.matchBlocked;
    case "local_program_unavailable": return copy.localUnavailable;
    case "endpoint_invalid": return copy.endpointInvalid;
    case "redirect_refused": return copy.redirectRefused;
    case "consent_required": return copy.consentRequired;
    case "key_required": return copy.keyRequired;
    case "key_invalid": return copy.keyInvalid;
    case "model_list_unavailable": return copy.modelListUnavailable;
    default: return copy.requestBlocked;
  }
}

export function ProviderControls({
  locale,
  developerMode: developerModeOverride,
  settings,
  onSettingsChange,
  onKeyStatusChange,
  compact = false,
  consentOpen,
  onConsentOpenChange,
}: Props) {
  const developerMode = developerModeOverride ?? (() => {
    try { return readDeveloperMode(window.localStorage); }
    catch { return false; }
  })();
  const copy = AI_PROVIDER_COPY[locale];
  const keyInput = useRef<HTMLInputElement>(null);
  const [providers, setProviders] = useState<ProviderRow[]>([]);
  const [keyStatus, setKeyStatus] = useState<KeyStatus>({ provider: settings.provider, set: false, last4: null });
  const [modelDraft, setModelDraft] = useState(settings.model);
  const [hasSecret, setHasSecret] = useState(false);
  const [fetchedModels, setFetchedModels] = useState<string[]>([]);
  const [endpointDraft, setEndpointDraft] = useState("");
  const [customEndpoint, setCustomEndpoint] = useState("");
  const [receipt, setReceipt] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localConsentOpen, setLocalConsentOpen] = useState(false);
  const isConsentOpen = consentOpen ?? localConsentOpen;
  const setConsentOpen = onConsentOpenChange ?? setLocalConsentOpen;

  const refreshProviders = async () => {
    const result = await invoke<ProviderList>("analytics_provider_list");
    setProviders(result.providers);
  };
  const refreshKey = async (providerId = settings.provider) => {
    const status = await invoke<KeyStatus>("analytics_provider_key_status", { providerId });
    setKeyStatus(status);
    if (providerId === settings.provider) {
      const current = await invoke<KeyStatus>("analytics_key_status");
      onKeyStatusChange?.(current);
    }
  };
  const refresh = async () => {
    try {
      const [next, list, key] = await Promise.all([
        invoke<ProviderPreferences>("analytics_settings_get"),
        invoke<ProviderList>("analytics_provider_list"),
        invoke<KeyStatus>("analytics_provider_key_status", { providerId: settings.provider }),
      ]);
      setProviders(list.providers);
      setKeyStatus(key);
      onSettingsChange(next);
      const currentKey = await invoke<KeyStatus>("analytics_key_status");
      onKeyStatusChange?.(currentKey);
    } catch {
      setError(locale === "th" ? "อ่านการตั้งค่าผู้ให้บริการไม่ได้" : "Could not read provider settings");
    }
  };

  useEffect(() => { void refresh(); }, [locale]);
  useEffect(() => {
    setModelDraft(settings.model);
    setFetchedModels([]);
    void refreshKey(settings.provider).catch(() => setKeyStatus({ provider: settings.provider, set: false, last4: null }));
  }, [settings.provider]);

  const visible = useMemo(() => visibleProviderRows(providers, developerMode && settings.show_untested), [providers, developerMode, settings.show_untested]);
  const selected = visible.find((row) => row.id === settings.provider);
  useEffect(() => { setEndpointDraft(selected?.configured_endpoint ?? ""); }, [selected?.id, selected?.configured_endpoint]);
  const localEndpointLabel = endpointClassLabel(selected, copy.localLabel);
  const models = useMemo(
    () => availableModels(settings.provider, selected?.models ?? [], fetchedModels),
    [settings.provider, selected, fetchedModels],
  );
  const canEnable = canEnableAi({
    model: settings.model,
    consent: settings.consent,
    requiresKey: selected?.requires_key ?? true,
    keySet: keyStatus.set,
  });
  const endpointEditable = selected?.endpoint_kind === "user" || selected?.endpoint_kind === "choice";
  const canFetchModels = selected?.model_list_fetch === true && settings.consent && (!selected.requires_key || keyStatus.set);

  async function savePatch(patch: Partial<ProviderPreferences>) {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_settings_patch", { patch });
      onSettingsChange(next);
      if (patch.show_untested !== undefined) await refreshProviders();
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function switchProvider(providerId: string) {
    if (!providerId || providerId === settings.provider) return;
    if (!window.confirm(copy.switchWarning)) return;
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_settings_patch", { patch: { provider: providerId } });
      onSettingsChange(next);
      setModelDraft(next.model);
      setFetchedModels([]);
      await Promise.all([refreshProviders(), refreshKey(providerId)]);
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function saveKey() {
    const value = keyInput.current?.value ?? "";
    if (!value) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("analytics_provider_key_set", { providerId: settings.provider, key: value });
      if (keyInput.current) keyInput.current.value = "";
      setHasSecret(false);
      await refresh();
    } catch (reason) {
      if (keyInput.current) keyInput.current.value = "";
      setHasSecret(false);
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function deleteKey() {
    if (!keyStatus.set || !window.confirm(locale === "th" ? "ลบ API key ของผู้ให้บริการนี้หรือไม่?" : "Delete this provider's API key?")) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("analytics_provider_key_delete", { providerId: settings.provider });
      await refresh();
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function saveEndpoint() {
    if (
      shouldWarnForEndpointChange(selected?.configured_endpoint, endpointDraft)
      && !window.confirm(copy.switchWarning)
    ) return;
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_provider_endpoint_set", {
        providerId: settings.provider,
        endpoint: endpointDraft,
      });
      onSettingsChange(next);
      await refreshProviders();
      await refreshKey(settings.provider);
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function registerEndpoint() {
    if (!customEndpoint.trim()) return;
    if (!window.confirm(copy.switchWarning)) return;
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<{ provider_id: string; settings: ProviderPreferences }>("analytics_custom_endpoint_register", { endpoint: customEndpoint });
      setCustomEndpoint("");
      onSettingsChange(result.settings);
      setModelDraft("");
      await refreshProviders();
      await refreshKey(result.provider_id);
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function fetchModels() {
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<string[]>("analytics_provider_models_fetch", { providerId: settings.provider });
      setFetchedModels(result);
    } catch (reason) {
      setError(friendlyError(reason, copy));
    } finally {
      setBusy(false);
    }
  }

  async function giveConsent() {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_consent_set", { consent: true });
      onSettingsChange(next);
      setConsentOpen(false);
    } catch {
      setError(locale === "th" ? "บันทึกความยินยอมไม่ได้" : "Could not save consent");
    } finally {
      setBusy(false);
    }
  }

  async function revokeConsent() {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_consent_set", { consent: false });
      onSettingsChange(next);
    } catch {
      setError(locale === "th" ? "ถอนความยินยอมไม่ได้" : "Could not revoke consent");
    } finally {
      setBusy(false);
    }
  }

  async function markVerified() {
    if (!selected || !/^[a-fA-F0-9]{64}$/.test(receipt)) return;
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<ProviderPreferences>("analytics_provider_verified_set", {
        providerId: selected.id,
        evidenceSha256: receipt,
      });
      onSettingsChange(next);
      setReceipt("");
      await refreshProviders();
    } catch {
      setError(locale === "th" ? "บันทึกหลักฐานการทดสอบไม่ได้" : "Could not record the live-test receipt");
    } finally {
      setBusy(false);
    }
  }

  const body = <div className="analytics-provider-controls">
    {developerMode && <label>
      <input
        type="checkbox"
        checked={settings.show_untested}
        disabled={busy}
        onChange={(event) => void savePatch({ show_untested: event.currentTarget.checked })}
      /> {copy.showUntested}
    </label>}
    {visible.length > 0 ? <label>{copy.provider}
      <select value={settings.provider} disabled={busy} onChange={(event) => void switchProvider(event.currentTarget.value)}>
        {visible.map((provider) => <option key={provider.id} value={provider.id}>
          {provider.label}{provider.verified ? "" : locale === "th" ? " · ยังไม่ได้ทดสอบจริง" : " · untested"}
        </option>)}
      </select>
    </label> : <p>{copy.noProvider}</p>}

    {selected && <>
      <p><strong>{selected.label}</strong>{selected.endpoint ? ` · ${selected.endpoint}` : ""}</p>
      {localEndpointLabel && <p className="analytics-provider-local">{localEndpointLabel} · {copy.localNote}</p>}

      {endpointEditable && <label>{copy.endpoint}
        <input
          type="url"
          value={endpointDraft || selected.configured_endpoint || ""}
          disabled={busy}
          onChange={(event) => setEndpointDraft(event.currentTarget.value)}
          onFocus={(event) => { if (!endpointDraft) setEndpointDraft(event.currentTarget.value); }}
        />
        <button type="button" disabled={busy || !endpointDraft} onClick={() => void saveEndpoint()}>{copy.saveEndpoint}</button>
      </label>}

      <label>{copy.key}
        <input
          ref={keyInput}
          type="password"
          autoComplete="off"
          placeholder={keyStatus.set ? `Set · ending ${keyStatus.last4 ?? "••••"}` : copy.keyNotSet}
          disabled={busy}
          onChange={(event) => setHasSecret(event.currentTarget.value.length > 0)}
        />
      </label>
      <div>
        <button type="button" disabled={busy || !hasSecret} onClick={() => void saveKey()}>{copy.saveKey}</button>
        <button type="button" disabled={busy || !keyStatus.set} onClick={() => void deleteKey()}>{copy.deleteKey}</button>
      </div>

      {settings.provider === "anthropic" ? <label>{copy.model}
        <select value={settings.model} disabled={busy} onChange={(event) => void savePatch({ model: event.currentTarget.value })}>
          {availableModels("anthropic").map((model) => <option key={model} value={model}>{model}</option>)}
        </select>
      </label> : <>
        <label>{copy.model}
          <input
            type="text"
            list="analytics-provider-model-list"
            value={modelDraft}
            placeholder={copy.typedModel}
            disabled={busy}
            onChange={(event) => setModelDraft(event.currentTarget.value)}
            onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void savePatch({ model: modelDraft }); } }}
            onBlur={() => { if (modelDraft !== settings.model) void savePatch({ model: modelDraft }); }}
          />
          <datalist id="analytics-provider-model-list">{models.map((model) => <option key={model} value={model} />)}</datalist>
        </label>
        {selected.model_list_fetch && <button type="button" disabled={busy || !canFetchModels} onClick={() => void fetchModels()}>{copy.fetchModels}</button>}
      </>}

      <button type="button" onClick={() => setConsentOpen(true)}>{copy.consent}</button>
      <p>{settings.consent ? copy.consentGranted : copy.consentMissing}</p>
      {settings.consent && <button type="button" disabled={busy} onClick={() => void revokeConsent()}>{copy.revokeConsent}</button>}
      <label>
        <input
          type="checkbox"
          checked={settings.ai_on}
          disabled={busy || (!settings.ai_on && !canEnable)}
          onChange={(event) => void savePatch({ ai_on: event.currentTarget.checked })}
        /> {copy.aiOn}
      </label>
      {!settings.model && <p>{copy.requestBlocked}</p>}
      <p>{copy.noSubscription}</p>
    </>}

    <label>{copy.customEndpoint}
      <input type="url" value={customEndpoint} placeholder="https://localhost.example/v1 or http://127.0.0.1:4000/v1" disabled={busy} onChange={(event) => setCustomEndpoint(event.currentTarget.value)} />
      <button type="button" disabled={busy || !customEndpoint.trim()} onClick={() => void registerEndpoint()}>{copy.addEndpoint}</button>
    </label>

    {developerMode && selected && !selected.verified && <div>
      <label>{copy.verifyReceipt}<input type="text" inputMode="text" value={receipt} maxLength={64} pattern="[a-fA-F0-9]{64}" disabled={busy} onChange={(event) => setReceipt(event.currentTarget.value)} /></label>
      <small>{copy.verifiedHelp}</small>
      <button type="button" disabled={busy || !/^[a-fA-F0-9]{64}$/.test(receipt)} onClick={() => void markVerified()}>{copy.markVerified}</button>
    </div>}
    {error && <p role="alert">{error}</p>}

    {isConsentOpen && selected && createPortal(<div className="analytics-consent-overlay" role="dialog" aria-modal="true" aria-labelledby="provider-consent-title"><section>
      <h2 id="provider-consent-title">{copy.consentTitle}</h2>
      <p><strong>{copy.consentHost}:</strong> {selected.label} · {selected.endpoint ?? copy.endpoint}</p>
      <p>{copy.consentContent}</p>
      <p>{locale === "th"
        ? "ไดเจสต์ที่ส่งยังมีคะแนนเลนเทียบกับค่าคาดหมาย แหล่งค่าคาดหมายและองค์ประกอบที่ใช้ พร้อมแหล่งผลแมตช์เมื่อทราบ ค่าคาดหมายอาจไม่มี และทุกการอ่านเลนใหม่แสดงว่า ‘ทดลอง · เกณฑ์ชั่วคราว’"
        : "The digest also includes lane scores against expectations, their sources and components, plus the match result source when known. An expectation may be unavailable, and every new lane reading is labelled ‘ทดลอง · เกณฑ์ชั่วคราว’."}</p>
      <p>{copy.consentRouting}</p>
      <p>{copy.consentExcluded}</p>
      <p>{copy.consentKey}</p>
      <p>{copy.consentPayment}</p>
      {localEndpointLabel && <p>{copy.localNote}</p>}
      <button type="button" onClick={() => setConsentOpen(false)}>{locale === "th" ? "กลับ" : "Back"}</button>
      <button type="button" disabled={busy} onClick={() => void giveConsent()}>{copy.grantConsent}</button>
    </section></div>, document.body)}
  </div>;

  return compact
    ? <details className="analytics-provider-details"><summary>{copy.pickerTitle}</summary>{body}</details>
    : <section className="analytics-provider-section"><h3>{copy.title}</h3>{body}</section>;
}
