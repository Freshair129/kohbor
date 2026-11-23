import { useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent, type MouseEvent } from "react";
import {
  accountSnapshotFromLegacy,
  guestAccountPort,
  type AccountActionResult,
  type AccountPort,
  type AccountSnapshot,
  type AccountView,
} from "./accountView";
import {
  ACCOUNT_COPY,
  accountCopy,
  buildAccountViewModel,
  countGraphemeClusters,
  type AccountLocale,
  type AccountScreenId,
} from "./accountViewModel";
import { useAccountSnapshot } from "./useAccount";
import "./account.css";

const dialogScreens: readonly AccountScreenId[] = ["A5b", "A7b", "A7c", "A8", "A8b"];

function formatCountdown(seconds: number): string {
  const minutes = Math.floor(seconds / 60).toString().padStart(2, "0");
  const remainder = (seconds % 60).toString().padStart(2, "0");
  return `${minutes}:${remainder}`;
}

function sessionDate(value: string | null, locale: AccountLocale): string | null {
  if (value === null) return null;
  const date = new Date(value);
  if (!Number.isFinite(date.getTime())) return null;
  return accountCopy(locale, "acct.signed_in_on", { date: new Intl.DateTimeFormat(locale === "th" ? "th-TH" : "en", { dateStyle: "medium" }).format(date) });
}

export function AccountPage({ account, locale, back, port: suppliedPort, liveMatch }: {
  account: AccountView;
  locale: AccountLocale;
  back: () => void;
  port?: AccountPort;
  liveMatch?: boolean;
}) {
  const port = suppliedPort ?? guestAccountPort;
  const portSnapshot = useAccountSnapshot(port);
  const authOnly = port.authOnly === true;
  const sourceSnapshot = suppliedPort ? portSnapshot : accountSnapshotFromLegacy(account);
  const snapshot: AccountSnapshot = {
    ...sourceSnapshot,
    liveMatch: liveMatch ?? sourceSnapshot.liveMatch,
  };
  const [now, setNow] = useState(() => Date.now());
  const [draftName, setDraftName] = useState("");
  const [editingName, setEditingName] = useState(false);
  const [operationError, setOperationError] = useState<keyof typeof ACCOUNT_COPY.en | null>(null);
  const [operationNotice, setOperationNotice] = useState<keyof typeof ACCOUNT_COPY.en | null>(null);
  const [copyAnnouncement, setCopyAnnouncement] = useState("");
  const [dialogOverride, setDialogOverride] = useState<AccountSnapshot["dialog"] | undefined>(undefined);
  const heading = useRef<HTMLHeadingElement | null>(null);
  const nameInput = useRef<HTMLInputElement | null>(null);
  const signInButton = useRef<HTMLButtonElement | null>(null);
  const safeAction = useRef<HTMLButtonElement | null>(null);
  const dialog = useRef<HTMLDivElement | null>(null);
  const dialogOpener = useRef<HTMLElement | null>(null);
  const returnFocus = useRef(false);
  const previousScreen = useRef<AccountScreenId | null>(null);
  const previousDialog = useRef<AccountSnapshot["dialog"]>(snapshot.dialog);
  const modelSnapshot = useMemo(() => ({
    ...snapshot,
    dialog: dialogOverride === undefined ? snapshot.dialog : dialogOverride,
  }), [snapshot, dialogOverride]);
  const model = buildAccountViewModel(modelSnapshot, locale, now);
  const t = (key: keyof typeof ACCOUNT_COPY.en) => accountCopy(locale, key);
  const hasPort = suppliedPort !== undefined;
  const showDialog = dialogScreens.includes(model.screenId);
  const graphemeCount = countGraphemeClusters(draftName, locale);
  const activeNameError = snapshot.state === "needs_display_name" || editingName ? snapshot.nameError : null;

  useEffect(() => {
    if (snapshot.handoffDeadline === null && snapshot.freshSignInDeadline === null && snapshot.rateLimitDeadline === null) return;
    const interval = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [snapshot.handoffDeadline, snapshot.freshSignInDeadline, snapshot.rateLimitDeadline]);

  useEffect(() => {
    if (snapshot.dialog !== previousDialog.current) {
      previousDialog.current = snapshot.dialog;
      setDialogOverride(undefined);
    }
  }, [snapshot.dialog]);

  useEffect(() => {
    const previous = previousScreen.current;
    previousScreen.current = model.screenId;
    if (previous === model.screenId || !document.hasFocus() || snapshot.liveMatch) return;
    if (model.screenId === "A4e" || (model.screenId === "A4" && activeNameError !== null)) {
      nameInput.current?.focus();
      return;
    }
    if (previous === "A2" && model.screenId === "A1" && returnFocus.current) {
      returnFocus.current = false;
      signInButton.current?.focus();
      return;
    }
    if (showDialog) {
      safeAction.current?.focus();
      return;
    }
    if (returnFocus.current && previous !== null && dialogScreens.includes(previous)) {
      returnFocus.current = false;
      dialogOpener.current?.focus();
      return;
    }
    heading.current?.focus();
  }, [model.screenId, showDialog, snapshot.liveMatch, activeNameError]);

  useEffect(() => {
    if (editingName && document.hasFocus() && !snapshot.liveMatch) nameInput.current?.focus();
  }, [editingName, snapshot.liveMatch]);

  useEffect(() => {
    if (!hasPort || model.screenId !== "A7" || snapshot.state !== "signed_in_online") return;
    void port.listSessions().catch(() => setOperationError("err.service.body"));
  }, [hasPort, model.screenId, snapshot.state, port]);

  const run = async (action: () => Promise<void | AccountActionResult>, failureKey?: keyof typeof ACCOUNT_COPY.en): Promise<boolean> => {
    setOperationError(null);
    setOperationNotice(null);
    try {
      const result = await action();
      if (result && !result.ok) { setOperationError(failureKey ?? "err.service.body"); return false; }
      return true;
    } catch {
      setOperationError(failureKey ?? "err.service.body");
      return false;
    }
  };

  const dismissDialog = () => {
    returnFocus.current = true;
    setDialogOverride(null);
  };

  const cancelSignIn = async () => {
    returnFocus.current = true;
    if (!(await run(() => port.cancelHandoff()))) returnFocus.current = false;
  };

  const openDialog = (value: NonNullable<AccountSnapshot["dialog"]>, event: MouseEvent<HTMLElement>) => {
    dialogOpener.current = event.currentTarget;
    returnFocus.current = false;
    setDialogOverride(value);
  };

  const closeDelete = () => {
    returnFocus.current = true;
    void run(() => port.keepAccount());
  };

  const submitName = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!hasPort) return;
    setOperationError(null);
    setOperationNotice(null);
    try {
      const result = await port.saveDisplayName(draftName);
      if (result && !result.ok) {
        if (port.getState().nameError === null) setOperationError("name.err.not_saved");
        if (document.hasFocus() && !snapshot.liveMatch) nameInput.current?.focus();
        return;
      }
      setEditingName(false);
      setDraftName("");
    } catch {
      setOperationError("name.err.not_saved");
      if (document.hasFocus() && !snapshot.liveMatch) nameInput.current?.focus();
    }
  };

  const copyFriendCode = async () => {
    if (!model.friendCode.value || model.friendCode.presentation !== "live") return;
    try {
      await navigator.clipboard.writeText(model.friendCode.value);
      setCopyAnnouncement(t("code.copied"));
    } catch {
      setCopyAnnouncement("");
    }
  };

  const dialogKeys = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      if (model.screenId === "A8" || model.screenId === "A8b") closeDelete();
      else dismissDialog();
      return;
    }
    if (event.key !== "Tab" || !dialog.current) return;
    const focusable = [...dialog.current.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), [href], [tabindex]:not([tabindex='-1'])")];
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
  };

  const codeBox = (allowCopy: boolean) => <div className={model.friendCode.className} role="group" aria-label={model.friendCode.label}>
    <output>{model.friendCode.value ?? "—"}</output>
    {model.friendCode.presentation === "unknown" && <span className="account-code-state">{t("code.none")}</span>}
    {model.friendCode.presentation === "cached" && <span className="account-code-state">{t("offline.title")}</span>}
    {model.friendCode.example && <span className="account-example">{t("code.example")}</span>}
    {allowCopy && model.friendCode.presentation === "live" && <button type="button" data-action="copy-code" onClick={() => void copyFriendCode()}>{t("code.copy")}</button>}
  </div>;

  const nameForm = (isDialog = false) => <form className="account-name-form" onSubmit={(event) => void submitName(event)}>
    <label htmlFor="account-display-name">{t("name.label")}</label>
    <input ref={nameInput} id="account-display-name" name="displayName" type="text" autoComplete="off"
      value={draftName} aria-describedby={`account-name-hint${activeNameError !== null ? " account-name-error" : ""}`} aria-invalid={activeNameError !== null}
      onChange={(event) => setDraftName(event.currentTarget.value)} />
    <div className="account-field-hint" id="account-name-hint"><span>{t("name.hint")}</span><span aria-hidden="true">{graphemeCount} / 32</span></div>
    {activeNameError !== null && <p className="account-inline-error" id="account-name-error" role="alert">{t(activeNameError === "empty" ? "name.err.empty" : activeNameError === "long" ? "name.err.long" : activeNameError === "refused" ? "name.err.refused" : "name.err.not_saved")}</p>}
    <p>{t("name.who_sees")}</p>
    <div className="account-actions"><button type="submit" disabled={!hasPort}>{t("name.save")}</button>
      {isDialog && <button type="button" onClick={() => setEditingName(false)}>{t("err.later")}</button>}</div>
  </form>;

  const actionDisabled = (disabled: boolean) => disabled || !hasPort;
  const errorCopyKey: keyof typeof ACCOUNT_COPY.en = model.screenId === "E6"
    ? snapshot.deleteError === "window" || (snapshot.freshSignInDeadline !== null && now >= snapshot.freshSignInDeadline) ? "delete.err.window" : "delete.err.mismatch"
    : model.screenId === "E1" ? "err.timeout.body"
      : model.screenId === "E2" ? "err.service.body"
        : model.screenId === "E3" ? "err.google.body" : model.screenId === "E5" ? "err.rate.body" : "revoked.body";

  return <section className="account-page" lang={locale} aria-labelledby="page-account-heading">
    <div className="account-page-scroll">
      <header className="account-page-heading"><div><small lang="en">ACCOUNT</small><h1 id="page-account-heading" ref={heading} tabIndex={-1}>{model.title}</h1></div>
      <button type="button" className="account-back" onClick={back}>{locale === "th" ? "← กลับหน้าสด" : "← Back to Live"}</button></header>
      <section className={`account-screen tone-${model.tone}`} aria-labelledby="page-account-heading" data-screen={model.screenId}>
      {(model.screenId === "A2" || model.screenId === "A3") && <p className="sr-only" role="status" aria-live="polite">{model.title}</p>}
      {model.screenId === "A1" || model.screenId === "A1b" || model.screenId === "A1c" ? <div className="account-card account-guest">
        <p>{t("guest.body")}</p>
        <div className="account-signin-promise">
          <button ref={signInButton} type="button" data-action="sign-in" disabled={actionDisabled(model.actions.signIn.disabled)} aria-describedby={`signin-promise signin-receives${model.actions.signIn.reason ? " signin-reason" : ""}`}
            onClick={() => void run(() => port.signIn())}>{t("signin.action")}</button>
          <p id="signin-promise">{t("signin.promise")}</p><p id="signin-receives">{t("signin.receives")}</p>
        </div>
        {model.invitedOnly && <p>{t("signin.alpha")}</p>}
        {model.actions.signIn.reason && <p id="signin-reason" className="account-disabled-reason">{model.screenId === "A1c" ? t("signin.in_match") : t("signin.not_open")}</p>}
        <p>{t("guest.no_google")}</p><p className="account-unknown-note">{t("code.none")}</p>
      </div> : null}

      {model.screenId === "A2" ? <div className="account-card">
        <ol className="account-steps"><li>{t("handoff.step1")}</li><li>{t("handoff.step2")}</li></ol>
        {model.handoffSecondsRemaining !== null && <p className="account-countdown">{accountCopy(locale, "handoff.waiting", { "m:ss": formatCountdown(model.handoffSecondsRemaining) })}</p>}
        <p>{t("handoff.no_password")}</p>
        <div className="account-actions">
          <button ref={safeAction} type="button" data-action="cancel-sign-in" onClick={() => void cancelSignIn()}>{t("handoff.cancel")}</button>
          <button type="button" data-action="reopen-browser" onClick={() => void run(() => port.signIn())}>{t("handoff.reopen")}</button>
        </div>
      </div> : null}

      {model.screenId === "A3" ? <div className="account-card">
        <ol className="account-steps"><li>{t("checking.step1")}</li><li>{t("checking.step2")}</li></ol>
        <p>{t("checking.close_tab")}</p>
      </div> : null}

      {(model.screenId === "A4" || model.screenId === "A4e") && <div className="account-card">
        <p>{t("name.not_google")}</p>{nameForm()}
        <button type="button" className="account-secondary" disabled={!hasPort} onClick={() => void run(() => port.signOutHere())}>{t("acct.signout")}</button>
      </div>}

      {model.screenId === "A5" && <div className="account-card">
        <p>{t("ready.nothing_shared")}</p><h2>{model.friendCode.label}</h2>{codeBox(true)}
        <p>{t("code.explain")}</p>
        <div className="account-actions"><button type="button" data-action="replace-code" disabled={actionDisabled(model.actions.replaceCode.disabled)}
          onClick={(event) => openDialog("replace_code", event)}>{t("code.new")}</button>
          <button type="button" data-action="finish-setup" disabled={actionDisabled(model.actions.finishSetup.disabled)} onClick={() => void run(() => port.finishSetup())}>{t("ready.done")}</button></div>
      </div>}

      {model.screenId === "A6" && <div className="account-card" role="status" aria-live="polite">
        <h2>{t("offline.head")}</h2><p>{t("offline.body")}</p>{codeBox(false)}<p>{t("offline.cached")}</p>
        <button type="button" data-action="retry-connection" disabled={!hasPort} onClick={() => void run(() => port.retryConnection())}>{t("offline.retry")}</button>
      </div>}

      {model.screenId === "A7" && authOnly && <div className="account-card" data-account-kind="supabase-auth">
        <h2>{locale === "th" ? "เข้าสู่ระบบด้วย Google แล้ว" : "Signed in with Google"}</h2>
        {snapshot.displayName !== null && <dl className="account-kv"><dt>{t("name.label")}</dt><dd>{snapshot.displayName}</dd></dl>}
        <p>{t("signin.receives")}</p>
        <button type="button" data-action="sign-out-here" disabled={actionDisabled(false)} onClick={() => void run(() => port.signOutHere())}>{t("acct.signout")}</button>
      </div>}

      {model.screenId === "A7" && !authOnly && <div className="account-page-grid">
        <section className="account-card">
          <h2>{t("acct.online")}</h2>
          {snapshot.displayName !== null && !editingName && <dl className="account-kv"><dt>{t("name.label")}</dt><dd>{snapshot.displayName}</dd></dl>}
          {editingName ? nameForm(true) : snapshot.displayName !== null ? <button type="button" className="account-secondary" onClick={() => { setDraftName(snapshot.displayName ?? ""); setEditingName(true); }}>{t("name.change")}</button> : null}
          <p className="account-data-note">{t("acct.signout.c")}</p>
          <button type="button" data-action="sign-out-here" disabled={actionDisabled(model.actions.signOutHere.disabled)} onClick={() => void run(() => port.signOutHere())}>{t("acct.signout")}</button>
        </section>
        <section className="account-card">
          <h2>{model.friendCode.label}</h2>{codeBox(true)}<p>{t("code.explain")}</p>
          <button type="button" data-action="replace-code" disabled={actionDisabled(model.actions.replaceCode.disabled)} onClick={(event) => openDialog("replace_code", event)}>{t("code.new")}</button>
          {model.actions.replaceCode.reason && <p className="account-disabled-reason">{t("code.new.limit")}</p>}
        </section>
        <section className="account-card account-sessions-card">
          <h2>{t("acct.devices")}</h2>
          {model.sessions === null ? <p>{t("acct.devices.unknown")}</p> : model.sessions.length === 0 ? <p>{t("acct.devices.none")}</p>
            : <ul className="account-sessions">{model.sessions.map((session, index) => <li key={`${session.thisDevice}-${index}`}>
              <span>{session.thisDevice ? t("acct.this_pc") : t("acct.other_device")}</span>
              {sessionDate(session.signedInOn, locale) && <small>{sessionDate(session.signedInOn, locale)}</small>}
            </li>)}</ul>}
          <div className="account-actions account-actions-stack"><button type="button" data-action="sign-out-everywhere" disabled={actionDisabled(model.actions.signOutEverywhere.disabled)} onClick={(event) => openDialog("sign_out_everywhere", event)}>{t("acct.signout_all")}</button>
            <button type="button" data-action="export" disabled={actionDisabled(model.actions.export.disabled)} onClick={(event) => openDialog("export", event)}>{t("export.action")}</button>
            <button type="button" className="account-danger" data-action="delete-account" disabled={actionDisabled(model.actions.deleteAccount.disabled)} aria-describedby={model.actions.deleteAccount.reason ? "delete-action-reason" : undefined} onClick={(event) => { dialogOpener.current = event.currentTarget; void run(() => port.beginDelete()); }}>{t("delete.action")}</button>
            {model.actions.deleteAccount.reason && <p id="delete-action-reason" className="account-disabled-reason">{model.actions.deleteAccount.reason}</p>}
          </div>
        </section>
      </div>}

      {model.screenId === "A5b" && <div className="account-dialog-backdrop"><div ref={dialog} className="account-dialog" role="dialog" aria-modal="true" aria-labelledby="account-dialog-heading" onKeyDown={dialogKeys}>
        <h2 id="account-dialog-heading">{t("code.new.title")}</h2>
        <p>{t("code.new.c1")}</p><p>{t("code.new.c2")}</p><p>{t("code.new.c3")}</p>
        {snapshot.codeReplacementsRemaining === 0 && <p className="account-disabled-reason">{t("code.new.limit")}</p>}
        {operationError && <p role="alert" className="account-inline-error">{t(operationError)}</p>}
        <div className="account-actions"><button type="button" ref={safeAction} onClick={dismissDialog}>{t("code.new.keep")}</button>
          <button type="button" data-action="confirm-replace-code" disabled={actionDisabled(model.actions.replaceCode.disabled)} onClick={() => void (async () => {
            if (await run(() => port.replaceFriendCode(), "code.new.failed")) {
              setDialogOverride(null);
              setOperationNotice("code.new.done");
            }
          })()}>{t("code.new")}</button></div>
      </div></div>}

      {model.screenId === "A7b" && <div className="account-dialog-backdrop"><div ref={dialog} className="account-dialog" role="dialog" aria-modal="true" aria-labelledby="account-dialog-heading" onKeyDown={dialogKeys}>
        <h2 id="account-dialog-heading">{t("acct.signout_all.title")}</h2><p>{t("acct.signout_all.c1")}</p><p>{t("acct.signout_all.c2")}</p><p>{t("acct.signout_all.c3")}</p>
        <div className="account-actions"><button type="button" ref={safeAction} onClick={dismissDialog}>{t("err.later")}</button>
          <button type="button" data-action="confirm-sign-out-everywhere" disabled={actionDisabled(model.actions.signOutEverywhere.disabled)} onClick={() => void (async () => {
            if (await run(() => port.signOutEverywhere())) setDialogOverride(null);
          })()}>{t("acct.signout_all")}</button></div>
      </div></div>}

      {model.screenId === "A7c" && <div className="account-dialog-backdrop"><div ref={dialog} className="account-dialog" role="dialog" aria-modal="true" aria-labelledby="account-dialog-heading" onKeyDown={dialogKeys}>
        <h2 id="account-dialog-heading">{t("acct.data")}</h2><p>{t("export.body")}</p><ul><li>{t("export.i1")}</li><li>{t("export.i2")}</li><li>{t("export.i3")}</li></ul><p>{t("export.warn")}</p>
        {operationError && <p role="alert" className="account-inline-error">{t(operationError)}</p>}
        <div className="account-actions"><button type="button" ref={safeAction} onClick={dismissDialog}>{t("err.later")}</button>
          <button type="button" data-action="export" disabled={actionDisabled(model.actions.export.disabled)} onClick={() => void (async () => {
            if (await run(() => port.exportAccount(), "export.failed")) setDialogOverride(null);
          })()}>{t("export.choose")}</button></div>
      </div></div>}

      {model.screenId === "A8" && <div className="account-dialog-backdrop"><div ref={dialog} className="account-dialog account-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="account-dialog-heading" onKeyDown={dialogKeys}>
        <h2 id="account-dialog-heading">{t("delete.title")}</h2>
        {snapshot.deleteStep === "reauth" ? <>
          <p>{t("delete.reauth")}</p>
          {model.handoffSecondsRemaining !== null && <p className="account-countdown">{accountCopy(locale, "handoff.waiting", { "m:ss": formatCountdown(model.handoffSecondsRemaining) })}</p>}
          <div className="account-actions"><button ref={safeAction} type="button" onClick={() => void run(() => port.cancelHandoff())}>{t("handoff.cancel")}</button>
            <button type="button" disabled={actionDisabled(model.actions.continueDelete.disabled)} aria-describedby={snapshot.liveMatch ? "delete-action-reason" : undefined} onClick={() => void run(() => port.deleteWithFreshSignIn())}>{t("delete.reauth.action")}</button></div>
          {snapshot.liveMatch && <p id="delete-action-reason" className="account-disabled-reason">{t("action.in_match")}</p>}
        </> : <>
          <p className="account-danger-text">{t("delete.undo")}</p><h3>{t("delete.gone")}</h3><ul><li>{t("delete.g1")}</li><li>{t("delete.g2")}</li><li>{t("delete.g3")}</li><li>{t("delete.g4")}</li></ul>
          <h3>{t("delete.stays")}</h3><ul><li>{t("delete.s1")}</li><li>{t("delete.s2")}</li></ul><p>{t("delete.reauth")}</p>
          {model.actions.deleteAccount.reason && <p className="account-disabled-reason">{model.actions.deleteAccount.reason}</p>}
          <div className="account-actions"><button ref={safeAction} type="button" onClick={closeDelete}>{t("delete.keep")}</button>
            <button type="button" disabled={actionDisabled(model.actions.continueDelete.disabled)} onClick={() => void run(() => port.deleteWithFreshSignIn())}>{t("delete.reauth.action")}</button></div>
          {snapshot.liveMatch && <p className="account-disabled-reason">{t("action.in_match")}</p>}
        </>}
      </div></div>}

      {model.screenId === "A8b" && <div className="account-dialog-backdrop"><div ref={dialog} className="account-dialog account-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="account-dialog-heading" onKeyDown={dialogKeys}>
        <h2 id="account-dialog-heading">{t("delete.confirm.title")}</h2>
        {model.deleteSecondsRemaining !== null && <p className="account-countdown">{accountCopy(locale, "delete.confirm.window", { "m:ss": formatCountdown(model.deleteSecondsRemaining) })}</p>}
        <p>{t("delete.confirm.body")}</p>{model.actions.confirmDelete.reason && <p className="account-disabled-reason">{model.actions.confirmDelete.reason}</p>}
        {operationError && <p role="alert" className="account-inline-error">{t(operationError)}</p>}
        <div className="account-actions"><button type="button" ref={safeAction} onClick={closeDelete}>{t("delete.keep")}</button>
          <button type="button" className="account-danger" data-action="confirm-delete" disabled={actionDisabled(model.actions.confirmDelete.disabled)} aria-describedby={model.actions.confirmDelete.reason ? "delete-confirm-reason" : undefined} onClick={() => void run(() => port.confirmDelete(), "delete.failed")}>{t("delete.final")}</button></div>
        {model.actions.confirmDelete.reason && <p id="delete-confirm-reason" className="account-disabled-reason">{model.actions.confirmDelete.reason}</p>}
      </div></div>}

      {model.screenId === "A6" ? null : null}
      {model.screenId === "E1" || model.screenId === "E2" || model.screenId === "E3" || model.screenId === "E5" || model.screenId === "E6" || model.screenId === "X1" ? <div className="account-card account-notice"
        role={model.announcementRole} aria-live={model.announcementRole === "alert" ? "assertive" : "polite"}>
        <span className="account-notice-glyph" aria-hidden="true">{model.tone === "error" ? "▲" : "!"}</span><p>{model.screenId === "E1" ? t("err.timeout.body") : model.screenId === "E2" ? t("err.service.body") : model.screenId === "E3" ? t("err.google.body") : model.screenId === "E5" ? t("err.rate.body") : model.screenId === "E6" ? t(errorCopyKey) : t("revoked.body")}</p>
        {model.screenId === "E3" && model.invitedOnly && <p>{t("err.google.alpha")}</p>}
        {model.screenId === "E5" && model.rateLimitSecondsRemaining !== null && <p>{accountCopy(locale, "err.rate.wait", { "m:ss": formatCountdown(model.rateLimitSecondsRemaining) })}</p>}
        <div className="account-actions">
          {model.screenId === "E6" && <button ref={safeAction} type="button" onClick={() => void run(() => port.keepAccount())}>{t("err.later")}</button>}
          {model.screenId === "E2" ? <button type="button" disabled={!hasPort} onClick={() => void run(() => port.retryConnection())}>{t("err.retry")}</button>
            : <button ref={model.screenId === "E6" ? undefined : safeAction} type="button" data-action="retry" disabled={actionDisabled(model.actions.signIn.disabled)} onClick={() => void run(() => model.screenId === "E6" ? port.deleteWithFreshSignIn() : port.signIn())}>{t("err.retry")}</button>}
        </div>
      </div> : null}

      {model.screenId === "X2" && <div className="account-card account-notice" role="status" aria-live="polite"><span className="account-notice-glyph" aria-hidden="true">!</span><p>{t("newdevice.body")}</p></div>}

      {model.screenId === "A1" && snapshot.notice === "deleted" && <p className="account-inline-success" role="status">{t("delete.done")}</p>}
      {snapshot.notice === "cancelled" && model.screenId === "A1" && <p className="account-inline-status" role="status">{t("handoff.cancelled")}</p>}
      {model.friendRequestFailure !== null && <p className="account-inline-error" role="alert">{model.friendRequestFailure}</p>}
      {operationError && model.screenId !== "A5b" && model.screenId !== "A7c" && model.screenId !== "A8b" && <p className="account-inline-error" role="alert">{t(operationError)}</p>}
      {operationNotice && <p className="account-inline-status" role="status">{t(operationNotice)}</p>}
      {copyAnnouncement && <p className="account-copy-announcement" role="status" aria-live="polite">{copyAnnouncement}</p>}
      {model.screenId === "A7" && model.friendCode.presentation === "cached" && <p className="account-inline-status">{t("offline.cached")}</p>}
      {model.screenId === "A7" && snapshot.liveMatch && <p className="account-disabled-reason">{t("action.in_match")}</p>}
      {model.screenId === "E1" || model.screenId === "E3" || model.screenId === "E5" || model.screenId === "E6" ? <p className="account-error-caption">{t("err.later")}</p> : null}
      </section>
    </div>
  </section>;
}

export function FriendsPage({ locale, back }: { locale: "th" | "en"; back: () => void }) {
  const th = locale === "th";
  return <section className="friends-page" lang={locale} aria-labelledby="page-friends-heading">
    <div className="friends-page-scroll">
      <header className="account-page-heading"><div><small>FRIENDS</small><h1 id="page-friends-heading">{th ? "เพื่อน" : "Friends"}</h1>
      <p>{th ? "ยังไม่มีข้อมูลจำนวนเพื่อน จึงไม่แสดงเป็นศูนย์" : "Friend count is unknown."}</p></div><button type="button" onClick={back}>{th ? "← กลับหน้าสด" : "← Back to Live"}</button></header>
      <div className="friends-page-grid"><aside className="account-card"><h2>{th ? "คนที่คุณเลือก" : "People you choose"}</h2>
      <p>{th ? "เพื่อนจะอยู่ตรงนี้เมื่อบริการพร้อม" : "Your friends will appear here when the service is available."}</p><hr />
      <button type="button" disabled>{th ? "เพิ่มเพื่อน" : "Add friend"}</button><p>{th ? "เร็วๆ นี้ — ยังไม่เปิดบริการ" : "Coming soon — service unavailable"}</p>
      <small>{accountCopy(locale, "signin.receives")}</small></aside>
      <section className="account-card friends-empty"><span className="friends-empty-mark" aria-hidden="true">—</span><small>{th ? "เร็วๆ นี้" : "Coming soon"}</small>
        <h2>{th ? "พื้นที่สำหรับเพื่อนของคุณ" : "A place for your friends"}</h2><p>{th ? "ยังไม่เปิดระบบเพื่อน ไม่มีรายชื่อหรือสถานะที่ต้องเดา" : "Friends are not available yet. No names or presence are inferred."}</p></section>
      </div>
    </div>
  </section>;
}
