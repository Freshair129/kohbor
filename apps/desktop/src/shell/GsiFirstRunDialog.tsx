import { useRef, useState } from "react";
import { gsiFirstRunCopy, reason, type Locale } from "../copy";
import type { RuntimeStatus } from "../types";
import { Dialog } from "../shared/Dialog";
import { gsiSetupFailure, isConfigConflict } from "./gsiSetupModel";
import "./gsiFirstRunDialog.css";

type Step = "intro" | "done" | "conflict";

export function GsiFirstRunDialog({ locale, status, setup, onDecline, onClose, onAccepted }: {
  locale: Locale;
  status: RuntimeStatus;
  setup: (replace: boolean) => Promise<string | null>;
  onDecline: () => void;
  onClose: () => void;
  onAccepted: () => void;
}) {
  const text = gsiFirstRunCopy[locale];
  const busyRef = useRef(false);
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState<Step>("intro");
  const [failure, setFailure] = useState<string | null>(null);

  const close = () => {
    if (busyRef.current) return;
    if (step === "intro") onDecline();
    else onClose();
  };

  const connect = async () => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const result = await setup(false);
      if (result === "GSI_CONFIG_CONFLICT" || isConfigConflict(status)) {
        setStep("conflict");
        return;
      }
      if (result !== null) {
        const code = gsiSetupFailure(status, result) ?? result;
        setFailure(reason(code, locale));
        return;
      }
      onAccepted();
      setStep("done");
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  return (
    <Dialog title={step === "done" ? text.done : text.title} closeLabel={text.closeLabel} onClose={close} className="gsi-first-run-dialog">
      {step === "intro" && (
        <>
          <p>{text.body}</p>
          {failure && <p className="gsi-error" role="alert">{failure}</p>}
          <div className="gsi-actions">
            <button type="button" className="gsi-primary" disabled={busy} aria-busy={busy} onClick={() => void connect()}>
              {busy ? text.busy : failure ? text.retry : text.connect}
            </button>
            <button type="button" disabled={busy} onClick={close}>{text.later}</button>
          </div>
        </>
      )}
      {step === "done" && (
        <>
          <p role="status">{text.done}</p>
          <p>{text.restart}</p>
          <div className="gsi-actions"><button type="button" className="gsi-primary" onClick={onClose}>{text.okay}</button></div>
        </>
      )}
      {step === "conflict" && (
        <>
          <p role="alert">{reason("GSI_CONFIG_CONFLICT", locale)}</p>
          <p>{text.conflict}</p>
          <div className="gsi-actions"><button type="button" onClick={onClose}>{text.close}</button></div>
        </>
      )}
    </Dialog>
  );
}
