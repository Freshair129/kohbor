import { useState } from "react";
import { gsiCopy, gsiNeedsAttention, gsiPhase, gsiSetupFailure, gsiStateLabel, isConfigConflict } from "./gsiSetupModel";
import { reason } from "../copy";
import type { RuntimeStatus } from "../types";
import "./gsiSetup.css";

/**
 * Visible on the main (live) page. Shows where the Dota 2 connection stands and, until data is really
 * arriving, a clear button that runs the same gsi_setup flow as Settings (with the replace confirmation).
 */
export function GsiSetupCard({ status, locale, setup }: {
  status: RuntimeStatus | null;
  locale: "th" | "en";
  setup: (replace: boolean) => Promise<string | null>;
}) {
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const [installed, setInstalled] = useState(false);
  const phase = gsiPhase(status);
  if (phase === null) return null;
  const text = gsiCopy(locale);
  const conflict = isConfigConflict(status);

  const run = async (replace: boolean) => {
    setBusy(true);
    setFailed(null);
    setConfirming(false);
    const error = await setup(replace);
    setFailed(error);
    setInstalled(error === null);
    setBusy(false);
  };

  if (!gsiNeedsAttention(phase)) {
    return <p className="gsi-card gsi-ok" role="status"><span aria-hidden="true">●</span> {gsiStateLabel("receiving", locale)}</p>;
  }
  const failure = gsiSetupFailure(status, failed);
  return (
    <section className={`gsi-card gsi-${phase}`} aria-label={text.title}>
      <div className="gsi-text">
        <strong>{text.title}</strong>
        <span role="status">{gsiStateLabel(phase, locale)}</span>
        {conflict && <span>{text.conflict}</span>}
        {installed && failure === null && <span role="status">{text.done}</span>}
        {failure !== null && <span className="gsi-error" role="alert">{reason(failure, locale)}</span>}
      </div>
      <div className="gsi-actions">
        {conflict ? (
          confirming ? (
            <>
              <button type="button" disabled={busy} onClick={() => void run(true)}>{text.confirmReplace}</button>
              <button type="button" disabled={busy} onClick={() => setConfirming(false)}>{text.cancel}</button>
            </>
          ) : (
            <button type="button" disabled={busy} onClick={() => setConfirming(true)}>{text.replace}</button>
          )
        ) : (
          <button type="button" className="gsi-primary" disabled={busy} aria-busy={busy} onClick={() => void run(false)}>
            {busy ? text.busy : phase === "not_installed" ? text.action : text.actionAgain}
          </button>
        )}
      </div>
    </section>
  );
}
