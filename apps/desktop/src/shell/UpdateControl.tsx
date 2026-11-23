import { useEffect, useRef, useState } from "react";
import type { Locale } from "../copy";
import type { AppVersionView } from "./appVersionModel";
import { createUpdateConfirmFlow, releasePageMessage, updateView } from "./updateModel";
import type { UpdateStatusSnapshot } from "./updateStatusStore";
import { updateCopy } from "./updateCopy";

export function UpdateControl({
  appVersion,
  locale,
  snapshot,
  check,
  install,
  openReleasePage,
}: {
  appVersion: AppVersionView;
  locale: Locale;
  snapshot: UpdateStatusSnapshot;
  check: () => Promise<void>;
  install: () => Promise<void>;
  openReleasePage: () => Promise<void>;
}) {
  const text = updateCopy(locale);
  const view = updateView(snapshot.reply, locale);
  const confirmation = useRef(createUpdateConfirmFlow());
  const [confirming, setConfirming] = useState(false);
  const currentVersion = snapshot.reply?.current_version ?? appVersion.version;
  const availableVersion = snapshot.reply?.available?.version ?? "";
  const disabledReason = snapshot.pending ? text.requestPending : (view.detail || view.headline);
  const buttonDisabled = snapshot.pending !== null;
  const canCheck = view.canCheck && !buttonDisabled;
  const canDownload = view.canDownload && !buttonDisabled && snapshot.download !== undefined;
  const canInstall = view.canInstall && !buttonDisabled;
  const matchBlocked = snapshot.reply?.match_active === true || snapshot.reply?.reason === "MATCH_ACTIVE";
  const canOpenPage = !buttonDisabled && !matchBlocked;
  const pageMessage = releasePageMessage(snapshot.page ?? null, locale);
  // Without a configured updater backend (this build) the only live action is opening the release page.
  const showUpdaterStates = snapshot.reply !== null && snapshot.reply.state !== "not_configured";

  useEffect(() => {
    if (view.canInstall) return;
    confirmation.current.cancel();
    setConfirming(false);
  }, [view.canInstall]);

  const beginConfirmation = () => {
    if (!canInstall) return;
    confirmation.current.begin();
    setConfirming(confirmation.current.isOpen());
  };

  const cancelConfirmation = () => {
    confirmation.current.cancel();
    setConfirming(false);
  };

  const confirmInstallation = () => {
    if (!canInstall || !confirming) return;
    const request = confirmation.current.confirm(install);
    setConfirming(confirmation.current.isOpen());
    void request;
  };

  return (
    <section className="shell-update-control" data-tone={view.tone} aria-label={text.label}>
      <span className="shell-update-version">{text.currentVersion}: {currentVersion}</span>
      {showUpdaterStates && (
        <div className="shell-update-state" id="shell-update-state" role="status">
          <strong>{view.headline}</strong>
          {view.detail && <span className="shell-update-detail">{view.detail}</span>}
          {snapshot.reply?.state === "downloading" && snapshot.reply.download_progress !== null && (
            <progress max={100} value={snapshot.reply.download_progress} aria-label={view.headline} />
          )}
          {snapshot.pending && <span className="shell-update-detail">{text.requestPending}</span>}
        </div>
      )}
      {snapshot.error && <span className="shell-update-error" role="alert">{text.requestFailed}</span>}
      {pageMessage && <span className="shell-update-detail" role="status">{pageMessage}</span>}
      <span className="shell-update-a11y" id="shell-update-disabled-reason">{matchBlocked ? text.matchActive : disabledReason}</span>
      <div className="shell-update-actions">
        {showUpdaterStates && (
          <label className="shell-update-auto-check" title={text.autoCheckHint}>
            <input
              type="checkbox"
              checked={snapshot.automaticChecksEnabled ?? false}
              disabled={buttonDisabled || matchBlocked}
              onChange={(event) => { void snapshot.setAutomaticChecksEnabled?.(event.currentTarget.checked); }}
            />
            {text.autoCheck}
          </label>
        )}
        <button
          type="button"
          disabled={!canOpenPage}
          aria-describedby={!canOpenPage ? "shell-update-disabled-reason" : undefined}
          title={text.openPageHint}
          onClick={() => { void openReleasePage(); }}
        >
          {text.openPage}
        </button>
        {showUpdaterStates && <button
          type="button"
          disabled={!canCheck}
          aria-describedby={!canCheck ? "shell-update-disabled-reason" : undefined}
          onClick={() => { void check(); }}
        >
          {text.check}
        </button>}
        {view.canDownload && (
          <button
            type="button"
            disabled={!canDownload}
            aria-describedby={!canDownload ? "shell-update-disabled-reason" : undefined}
            onClick={() => { void snapshot.download?.(); }}
          >
            {text.download}
          </button>
        )}
        {view.canInstall && !confirming && (
          <button
            type="button"
            disabled={!canInstall}
            aria-describedby={!canInstall ? "shell-update-disabled-reason" : undefined}
            onClick={beginConfirmation}
          >
            {text.install}
          </button>
        )}
        {view.canInstall && confirming && (
          <div className="shell-update-confirm" role="group" aria-label={text.confirmInstall(availableVersion)}>
            <span>{text.confirmInstall(availableVersion)}</span>
            <button
              type="button"
              disabled={!canInstall}
              aria-describedby={!canInstall ? "shell-update-disabled-reason" : undefined}
              onClick={confirmInstallation}
            >
              {text.confirm}
            </button>
            <button type="button" onClick={cancelConfirmation}>{text.cancel}</button>
          </div>
        )}
      </div>
    </section>
  );
}
