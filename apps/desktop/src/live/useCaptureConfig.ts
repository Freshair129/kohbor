import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import {
  parseCaptureConfig,
  type CaptureConfig,
  type CaptureConfigPatch,
  type CaptureDisplayRate,
} from "./captureConfig";
import { liveMockRequested } from "./mockMode";

function errorText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error && error.message) return error.message;
  return String(error);
}

export function useCaptureConfig() {
  const mock = liveMockRequested();
  const [config, setConfig] = useState<CaptureConfig | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [proposalDismissed, setProposalDismissed] = useState(false);

  async function readConfig(): Promise<CaptureConfig> {
    const payload = mock
      ? await (await import("./mock")).getMockCaptureConfig()
      : await invoke<unknown>("capture_get_config");
    const parsed = parseCaptureConfig(payload);
    if (!parsed) throw new Error("CAPTURE_CONFIG_INVALID");
    return parsed;
  }

  async function refresh(clearError = true) {
    try {
      const next = await readConfig();
      setConfig(next);
      if (clearError) setError(null);
      return next;
    } catch (failure) {
      setError(errorText(failure));
      return null;
    }
  }

  useEffect(() => {
    void refresh();
  }, [mock]);

  useEffect(() => {
    if (config?.proposal_status !== "searching") return;
    const timer = window.setInterval(() => {
      void refresh(false);
    }, 500);
    return () => window.clearInterval(timer);
  }, [config?.proposal_status, mock]);

  async function runAction(command: string, args?: Record<string, unknown>) {
    setBusy(true);
    setError(null);
    let actionFailure: string | null = null;
    let commandSucceeded = false;

    try {
      const payload = mock
        ? await (await import("./mock")).invokeMockCaptureConfig(command, args)
        : await invoke<unknown>(command, args);
      const returnedConfig = parseCaptureConfig(payload);
      if (!returnedConfig) throw new Error("CAPTURE_CONFIG_INVALID");
      setConfig(returnedConfig);
      commandSucceeded = true;
    } catch (failure) {
      actionFailure = errorText(failure);
    }

    try {
      setConfig(await readConfig());
    } catch (failure) {
      actionFailure ??= errorText(failure);
    }

    setError(actionFailure);
    setBusy(false);
    return commandSucceeded;
  }

  return {
    config,
    error,
    busy,
    proposalDismissed,
    setCrop: (crop: NonNullable<CaptureConfigPatch["crop"]>) =>
      runAction("capture_set_config", { patch: { crop } satisfies CaptureConfigPatch }),
    setDisplayRate: (display_fps: CaptureDisplayRate) =>
      runAction("capture_set_config", { patch: { display_fps } satisfies CaptureConfigPatch }),
    setPaused: (paused: boolean) => runAction("capture_set_config", { patch: { paused } satisfies CaptureConfigPatch }),
    resetCrop: () => runAction("capture_reset_crop"),
    proposeCrop: () => {
      setProposalDismissed(false);
      return runAction("capture_propose_crop");
    },
    acceptProposal: () => {
      setProposalDismissed(false);
      return runAction("capture_accept_proposal");
    },
    confirmCrop: () => runAction("capture_confirm_crop"),
    retryCapture: () => runAction("capture_retry"),
    dismissProposal: () => setProposalDismissed(true),
  };
}
