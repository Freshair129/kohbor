import { useState } from "react";
import { copy, OWNED_CONFIG_FILE, reason, seconds, type Locale } from "./copy";
import type { RuntimeStatus } from "./types";

type Action = "setup" | "replace";

interface Step {
  tone: "action" | "info" | "warn";
  title: string;
  body: string;
  action?: { kind: Action; label: string };
}

/** The single most useful next step for the current state, or null when nothing is pending. */
export function nextStep(status: RuntimeStatus, locale: Locale): Step | null {
  const text = copy(locale).guide;
  const i = status.intake;
  switch (i.state) {
    case "not_configured":
      return {
        tone: "action",
        title: text.setupTitle,
        body: text.setupBody(OWNED_CONFIG_FILE),
        action: { kind: "setup", label: text.setupAction },
      };
    case "unavailable":
      switch (i.reason) {
        case "GSI_CONFIG_CONFLICT":
          return {
            tone: "warn",
            title: text.configConflictTitle,
            body: text.configConflictBody(OWNED_CONFIG_FILE),
            action: { kind: "replace", label: text.replaceAction },
          };
        case "GSI_GAME_NOT_FOUND":
          return {
            tone: "warn",
            title: text.gameNotFoundTitle,
            body: text.gameNotFoundBody,
            action: { kind: "setup", label: text.tryAgain },
          };
        case "GSI_PORT_UNAVAILABLE":
          return {
            tone: "warn",
            title: text.portUnavailableTitle,
            body: text.portUnavailableBody,
            action: { kind: "setup", label: text.tryAgain },
          };
        case "GSI_PORT_EXHAUSTED":
          return {
            tone: "warn",
            title: text.portsExhaustedTitle,
            body: text.portsExhaustedBody,
            action: { kind: "setup", label: text.tryAgain },
          };
        case "GSI_CONFIG_WRITE_FAILED":
          return {
            tone: "warn",
            title: text.writeFailedTitle,
            body: text.writeFailedBody,
            action: { kind: "setup", label: text.tryAgain },
          };
        case "GSI_SECRET_UNAVAILABLE":
          return {
            tone: "warn",
            title: text.secretUnavailableTitle,
            body: text.secretUnavailableBody,
          };
        default:
          return { tone: "warn", title: text.intakeFailedTitle, body: reason(i.reason, locale) };
      }
    case "listening":
      return {
        tone: "info",
        title: text.listeningTitle,
        body: text.listeningBody(i.port),
      };
    case "stale":
      return {
        tone: "info",
        title: text.staleTitle,
        body: text.staleBody(seconds(i.last_admitted_age_ms, locale)),
      };
    case "receiving":
      if (status.ownership.state === "unverified" || status.ownership.state === "mismatch") {
        return {
          tone: "warn",
          title: text.ownerUnverifiedTitle,
          body: text.ownerUnverifiedBody,
        };
      }
      return null;
    default:
      return null;
  }
}

interface Props {
  status: RuntimeStatus;
  locale: Locale;
  setup: (replace: boolean) => Promise<string | null>;
}

export function Guide({ status, locale, setup }: Props) {
  const text = copy(locale).guide;
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const step = nextStep(status, locale);
  if (!step) return null;

  const run = async (replace: boolean) => {
    setBusy(true);
    setFailed(null);
    setConfirming(false);
    setFailed(await setup(replace));
    setBusy(false);
  };
  const action = step.action;
  // The row already carries the reason once the host reports it as its state.
  const showFailure = failed !== null && failed !== status.intake.reason;

  const replacing = action?.kind === "replace" && confirming;

  return (
    <section className={`guide guide-${step.tone}`} aria-labelledby="guide-h">
      <div className="guide-text">
        <h2 id="guide-h">{step.title}</h2>
        {/* The confirmation takes the place of the explanation, so the page keeps its height. */}
        <p>
          {replacing
            ? text.replaceConfirm(OWNED_CONFIG_FILE)
            : step.body}
        </p>
        {showFailure && (
          <p className="error" role="alert">
            {reason(failed, locale)}
          </p>
        )}
      </div>
      {action && !replacing && (
        <button
          type="button"
          className="primary"
          disabled={busy}
          aria-busy={busy}
          onClick={() => (action.kind === "replace" ? setConfirming(true) : void run(false))}
        >
          {busy ? text.busy : action.label}
        </button>
      )}
      {replacing && (
        <div className="guide-actions" role="group" aria-label={text.replaceAria}>
          <button type="button" className="primary" disabled={busy} onClick={() => void run(true)}>
            {text.confirmReplace}
          </button>
          <button type="button" disabled={busy} onClick={() => setConfirming(false)}>
            {text.cancel}
          </button>
        </div>
      )}
    </section>
  );
}
