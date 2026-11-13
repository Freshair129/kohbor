import { useCallback, useEffect, useId, useRef, useState } from "react";
import { copy, packReason, type Locale } from "./copy";
import {
  buildInstallRequest,
  errorCode,
  formatBytes,
  isPlausibleSourcePath,
  newRequestId,
  normalizeSourcePath,
  packState,
  parseInspectResult,
  parseInstallResult,
  parseMediaListing,
  sortRecords,
  type ChosenActivationPolicy,
  type InstallResult,
  type MediaListing,
  type PackRecord,
} from "./packs";
import { Chip, type Tone } from "./Readiness";
import { mediaActivate, mediaInspect, mediaInstall, mediaList } from "./runtime";

const PENDING_POLL_MS = 3_000;

type ListState = "loading" | "ok" | "unreadable" | "failed";

interface Inspected {
  path: string;
  sha256: string;
  size: number;
  /** Reused if the install call fails in transit, so a retry is idempotent on the host. */
  requestId: string;
}

interface Notice {
  tone: "ok" | "warn";
  text: string;
  /** A "waiting" notice is dropped once the listing no longer shows this install as pending. */
  waitingInstallId?: string;
}

const STATE_TONE: Record<"active" | "pending" | "installed", Tone> = {
  active: "fresh",
  pending: "warn",
  installed: "unknown",
};

export function PackPanel({ locale, disabled = false }: { locale: Locale; disabled?: boolean }) {
  const text = copy(locale).packs;
  const pathId = useId();
  const hashId = useId();

  const [listing, setListing] = useState<MediaListing | null>(null);
  const [listState, setListState] = useState<ListState>("loading");
  const [listError, setListError] = useState<string | null>(null);
  const [listBusy, setListBusy] = useState(false);

  const [pathInput, setPathInput] = useState("");
  const [inspected, setInspected] = useState<Inspected | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [error, setError] = useState<string | null>(null);

  const alive = useRef(true);
  const actionLock = useRef(false);
  const listSeq = useRef(0);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    const seq = ++listSeq.current;
    setListBusy(true);
    try {
      const parsed = parseMediaListing(await mediaList());
      if (!alive.current || seq !== listSeq.current) return;
      if (parsed) {
        setListing(parsed);
        setListState("ok");
        setListError(null);
        // The chip in the list now carries the real state; an old "still waiting" line would contradict it.
        setNotice((current) =>
          current?.waitingInstallId && !parsed.pending.includes(current.waitingInstallId) ? null : current,
        );
      } else {
        // An unreadable answer must not leave an old "active" pack looking current.
        setListing(null);
        setListState("unreadable");
        setListError(null);
      }
    } catch (e) {
      if (!alive.current || seq !== listSeq.current) return;
      setListState("failed");
      setListError(errorCode(e));
    } finally {
      if (alive.current && seq === listSeq.current) setListBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const hasPending = (listing?.pending.length ?? 0) > 0;
  useEffect(() => {
    if (!hasPending) return;
    const id = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, PENDING_POLL_MS);
    return () => window.clearInterval(id);
  }, [hasPending, refresh]);

  const describe = useCallback(
    (result: InstallResult): Notice => {
      const id = result.record?.install_id ?? "";
      const base =
        result.status === "rejected"
          ? text.outcome.rejected
          : text.outcome[result.status](id);
      const detail =
        result.reason && !(result.status === "deferred" && result.reason === "ACTIVATION_DEFERRED")
          ? ` — ${packReason(result.reason, locale)}`
          : "";
      const tone = result.status === "installed" || result.status === "active" ? "ok" : "warn";
      return {
        tone,
        text: `${base}${detail}`,
        waitingInstallId: result.status === "deferred" ? id : undefined,
      };
    },
    [locale, text],
  );

  const runExclusive = useCallback(async (label: string, work: () => Promise<void>) => {
    if (actionLock.current) return;
    actionLock.current = true;
    setBusy(label);
    setNotice(null);
    setError(null);
    try {
      await work();
    } finally {
      actionLock.current = false;
      if (alive.current) setBusy(null);
    }
  }, []);

  const onPathChange = (value: string) => {
    setPathInput(value);
    // A hash read from another path must never be reused for this one.
    setInspected(null);
    setNotice(null);
    setError(null);
  };

  const onInspect = () =>
    void runExclusive("inspect", async () => {
      const path = normalizeSourcePath(pathInput);
      if (!isPlausibleSourcePath(path)) {
        setError(text.pathInvalid);
        return;
      }
      try {
        const result = parseInspectResult(await mediaInspect(path));
        if (!alive.current) return;
        if (!result) {
          setError(text.inspectUnreadable);
          return;
        }
        setInspected({ path, sha256: result.container_sha256, size: result.size_bytes, requestId: newRequestId() });
      } catch (e) {
        if (alive.current) setError(packReason(errorCode(e), locale));
      }
    });

  const onInstall = () =>
    void runExclusive("install", async () => {
      if (!inspected) return;
      const request = buildInstallRequest({
        request_id: inspected.requestId,
        source_path: inspected.path,
        expected_container_sha256: inspected.sha256,
      });
      if (!request) {
        setError(text.pathInvalid);
        return;
      }
      try {
        const result = parseInstallResult(await mediaInstall(request));
        if (!alive.current) return;
        if (!result) {
          setError(text.resultUnreadable);
          void refresh();
          return;
        }
        setNotice(describe(result));
        // The result is final for this request id, whether installed or rejected.
        setInspected(null);
        if (result.status !== "rejected") setPathInput("");
        void refresh();
      } catch (e) {
        // Transport failure: keep the inspected file and request id so a retry is the same request.
        if (alive.current) setError(packReason(errorCode(e), locale));
      }
    });

  const onActivate = (record: PackRecord, policy: ChosenActivationPolicy) =>
    void runExclusive(`activate:${record.install_id}`, async () => {
      try {
        const result = parseInstallResult(await mediaActivate(newRequestId(), record.install_id, policy));
        if (!alive.current) return;
        if (!result) setError(text.resultUnreadable);
        else setNotice(describe(result));
      } catch (e) {
        if (alive.current) setError(packReason(errorCode(e), locale));
      } finally {
        if (alive.current) void refresh();
      }
    });

  const actionsBlocked = disabled || busy !== null;
  const records = listing ? sortRecords(listing.records) : [];
  const listOutdated = listState === "failed";

  return (
    <section className="card pack-panel" aria-labelledby="packs-h">
      <div className="card-head">
        <h2 id="packs-h">{text.title}</h2>
        <button type="button" onClick={() => void refresh()} disabled={disabled || listBusy} aria-busy={listBusy}>
          {listBusy ? text.refreshing : text.refresh}
        </button>
      </div>
      <p className="audio-note">{text.readinessNote}</p>

      {listState === "loading" && (
        <p className="muted" role="status">
          {text.listLoading}
        </p>
      )}
      {listState === "unreadable" && (
        <p className="error" role="alert">
          {text.listUnreadable}
        </p>
      )}
      {listState === "failed" && (
        <p className="error" role="alert">
          {text.listFailed} — {packReason(listError, locale)}
        </p>
      )}

      {listing && (
        <div className={listOutdated ? "pack-body is-outdated" : "pack-body"}>
          <div className="pack-active">
            <span className="row-name">{text.activeLabel}</span>
            {listing.active ? (
              <>
                <Chip tone="fresh" label={text.states.active} />
                <span className="row-detail">
                  <strong>{listing.active.package_id}</strong> · {text.revision(listing.active.revision)} ·{" "}
                  <span className="mono">{listing.active.install_id}</span>
                </span>
              </>
            ) : (
              <>
                <Chip tone="unknown" label={text.noActive} />
                <span className="row-detail" />
              </>
            )}
          </div>

          <h3 className="pack-subhead">{text.listHeading}</h3>
          {records.length === 0 ? (
            <p className="muted">{text.noRecords}</p>
          ) : (
            <ul className="pack-list">
              {records.map((record) => {
                const state = packState(record, listing);
                const rowBusy = busy === `activate:${record.install_id}`;
                return (
                  <li key={record.install_id} className="pack-item">
                    <div className="pack-item-head">
                      <strong>{record.package_id}</strong>
                      <span className="muted">{text.revision(record.revision)}</span>
                      <Chip tone={STATE_TONE[state]} label={text.states[state]} />
                    </div>
                    <div className="pack-meta muted small">
                      <span className="mono">{record.install_id}</span>
                      <span className="mono" title={record.container_sha256}>
                        sha256:{record.container_sha256.slice(0, 12)}…
                      </span>
                      <span>{text.authority}</span>
                      <span>{text.publisher[record.publisher_claim]}</span>
                    </div>
                    {state === "installed" && (
                      <div className="pack-actions">
                        <button
                          type="button"
                          disabled={actionsBlocked}
                          aria-busy={rowBusy}
                          onClick={() => onActivate(record, "now_if_idle")}
                        >
                          {rowBusy ? text.activating : text.activateNow}
                        </button>
                        <button
                          type="button"
                          disabled={actionsBlocked}
                          onClick={() => onActivate(record, "after_match")}
                        >
                          {text.activateAfterMatch}
                        </button>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          {hasPending && <p className="audio-note">{text.pendingNote}</p>}
        </div>
      )}

      <div className="pack-install">
        <h3 className="pack-subhead">{text.installHeading}</h3>
        <label className="pack-label" htmlFor={pathId}>
          {text.pathLabel}
        </label>
        <div className="pack-path-row">
          <input
            id={pathId}
            className="pack-path mono"
            type="text"
            value={pathInput}
            spellCheck={false}
            autoComplete="off"
            disabled={disabled || busy !== null}
            placeholder={"C:\\…\\pack.zip"}
            onChange={(event) => onPathChange(event.currentTarget.value)}
          />
          <button
            type="button"
            disabled={actionsBlocked || pathInput.trim() === ""}
            aria-busy={busy === "inspect"}
            onClick={onInspect}
          >
            {busy === "inspect" ? text.inspecting : text.inspect}
          </button>
        </div>
        <p className="muted small">{text.pathHint}</p>

        {inspected && (
          <div className="pack-inspected">
            <p role="status">{text.inspected(formatBytes(inspected.size))}</p>
            <p className="muted small" id={hashId}>
              {text.hashLabel}
            </p>
            <code className="pack-hash mono" aria-labelledby={hashId}>
              {inspected.sha256}
            </code>
            <p className="audio-note">{text.installNote}</p>
            <div className="pack-actions">
              <button
                type="button"
                className="primary"
                disabled={actionsBlocked}
                aria-busy={busy === "install"}
                onClick={onInstall}
              >
                {busy === "install" ? text.installing : text.install}
              </button>
            </div>
          </div>
        )}
      </div>

      {notice && (
        <p className={notice.tone === "ok" ? "pack-notice" : "pack-notice pack-notice-warn"} role="status">
          {notice.text}
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
