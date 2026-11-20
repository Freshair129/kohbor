import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useRef, useState } from "react";
import { buildInstallRequest, errorCode, newRequestId, parseInstallResult } from "../packs";
import { Dialog } from "../shared/Dialog";
import { installDialogCopy } from "./InstallDialog.copy";
import {
  droppedZipPath,
  formatPackageBytes,
  installResultMessage,
  isZipPath,
  parseCancelPendingActivationResult,
  parsePackageSummary,
  pointInsideRect,
  readPackageFile,
  type InstallDialogLocale,
  type PackageSummary,
} from "./InstallDialog.model";
import "./InstallDialog.css";

type BusyAction = "summary" | "install" | "activate" | "cancel" | null;
type InstallResult = NonNullable<ReturnType<typeof parseInstallResult>>;
type PackageSource = { kind: "file"; file: File } | { kind: "drop"; path: string };

async function readPickerBytes(file: File): Promise<ArrayBuffer> {
  return readPackageFile(file, await invoke<unknown>("media_package_size_limit"));
}

/**
 * Standalone Packs-page surface; the P1 page owns mounting and supplies the
 * current match state so activation results can be explained in context.
 */
export interface InstallDialogProps {
  locale: InstallDialogLocale;
  matchInProgress: boolean;
  onClose: () => void;
  onInstalled?: () => void;
}

export function InstallDialog({ locale, matchInProgress, onClose, onInstalled }: InstallDialogProps) {
  const text = installDialogCopy[locale];
  const dropZone = useRef<HTMLDivElement | null>(null);
  const busyRef = useRef(false);
  const [busy, setBusy] = useState<BusyAction>(null);
  const [source, setSource] = useState<PackageSource | null>(null);
  const [summary, setSummary] = useState<PackageSummary | null>(null);
  const [installResult, setInstallResult] = useState<InstallResult | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const [dragTargeted, setDragTargeted] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reasonText = useCallback(
    (code: string) => text.rejectionReasons[code] ?? `${text.reasonPrefix}: ${code}`,
    [text.rejectionReasons, text.reasonPrefix],
  );
  const operationError = useCallback((reason: unknown) => {
    const code = errorCode(reason);
    return code === "UNKNOWN" ? text.operationFailed : reasonText(code);
  }, [reasonText, text.operationFailed]);

  const inspectSource = useCallback(async (selected: PackageSource) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy("summary");
    setSource(selected);
    setSummary(null);
    setInstallResult(null);
    setCancelled(false);
    setError(null);
    try {
      if (selected.kind === "file" && !isZipPath(selected.file.name)) {
        setError(text.unsupportedFile);
        return;
      }
      const value = selected.kind === "file"
        ? await invoke<unknown>("media_package_summary_bytes", await readPickerBytes(selected.file))
        : await invoke<unknown>("media_package_summary", { sourcePath: selected.path });
      const parsed = parsePackageSummary(value);
      if (parsed) setSummary(parsed);
      else setError(text.invalidSummary);
    } catch (reason) {
      setError(operationError(reason));
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  }, [operationError, text.invalidSummary, text.unsupportedFile]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "leave") {
        setDragTargeted(false);
        return;
      }
      const rect = dropZone.current?.getBoundingClientRect();
      const inside = rect !== undefined && pointInsideRect(
        event.payload.position,
        rect,
        window.devicePixelRatio,
      );
      setDragTargeted(event.payload.type !== "drop" && inside);
      if (event.payload.type !== "drop" || !inside || busyRef.current) return;
      setDragTargeted(false);
      const path = droppedZipPath(event.payload.paths);
      if (!path) {
        setError(text.unsupportedFile);
        return;
      }
      void inspectSource({ kind: "drop", path });
    }).then((removeListener) => {
      if (disposed) removeListener();
      else unlisten = removeListener;
    }).catch(() => {
      // The component can also be typechecked or previewed outside the Tauri host.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [inspectSource, text.unsupportedFile]);

  const install = useCallback(async () => {
    if (busyRef.current || !source || summary?.status !== "valid" || !summary.container_sha256) return;
    busyRef.current = true;
    setBusy("install");
    setError(null);
    setInstallResult(null);
    setCancelled(false);
    let installationAccepted = false;
    try {
      const requestId = newRequestId();
      const digest = summary.container_sha256;
      let response: unknown;
      if (source.kind === "file") {
        response = await invoke("media_install_bytes", await readPickerBytes(source.file), {
          headers: { "x-kohbor-request-id": requestId, "x-kohbor-container-sha256": digest },
        });
      } else {
        const request = buildInstallRequest({
          request_id: requestId,
          source_path: source.path,
          expected_container_sha256: digest,
        });
        if (!request) {
          setError(text.operationFailed);
          return;
        }
        response = await invoke("media_install", { request });
      }
      const installed = parseInstallResult(response);
      if (!installed) {
        setError(text.operationFailed);
        return;
      }
      if (installed.status === "rejected") {
        setInstallResult(installed);
        return;
      }
      installationAccepted = true;
      const record = installed.record;
      if (!record) {
        setError(text.operationFailed);
        return;
      }
      setBusy("activate");
      const activated = parseInstallResult(await invoke<unknown>("media_activate", {
        requestId: newRequestId(),
        installId: record.install_id,
        policy: "now_if_idle",
      }));
      if (!activated) {
        setError(text.operationFailed);
        setInstallResult(installed);
        return;
      }
      setInstallResult(activated);
    } catch (reason) {
      setError(operationError(reason));
    } finally {
      busyRef.current = false;
      setBusy(null);
      if (installationAccepted) {
        try { onInstalled?.(); } catch { /* The host result remains authoritative. */ }
      }
    }
  }, [onInstalled, operationError, source, summary, text.operationFailed]);

  const cancelPending = useCallback(async () => {
    const installId = installResult?.record?.install_id;
    if (!installId || busyRef.current) return;
    busyRef.current = true;
    setBusy("cancel");
    setError(null);
    try {
      const result = parseCancelPendingActivationResult(await invoke<unknown>(
        "media_cancel_pending_activation",
        { installId },
      ));
      if (!result) {
        setError(text.operationFailed);
      } else if (result.cancelled) {
        setCancelled(true);
        try { onInstalled?.(); } catch { /* The host result remains authoritative. */ }
      } else {
        setError(text.noLongerPending);
      }
    } catch (reason) {
      setError(operationError(reason));
    } finally {
      busyRef.current = false;
      setBusy(null);
    }
  }, [installResult, onInstalled, operationError, text.noLongerPending, text.operationFailed]);

  const resultMessage = installResult
    ? `${installResultMessage(installResult.status, locale)}${installResult.reason ? ` ${reasonText(installResult.reason)}` : ""}`
    : null;

  return (
    <Dialog title={text.title} closeLabel={text.close} onClose={onClose} className="pack-install-dialog">
      <div className="pack-install-actions">
        <input
          type="file"
          accept=".zip,application/zip"
          aria-label={text.chooseFile}
          disabled={busy !== null}
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file) void inspectSource({ kind: "file", file });
          }}
        />
        {source && <span>{source.kind === "file" ? source.file.name : source.path.split(/[\\/]/).at(-1)}</span>}
      </div>

      <div
        className="pack-install-drop-zone"
        data-active={dragTargeted}
        ref={dropZone}
        aria-label={text.dropTitle}
      >
        <strong>{text.dropTitle}</strong>
        <span>{text.dropHint}</span>
      </div>

      {busy && <p className="pack-install-result" role="status">{text[busy === "summary" ? "checking" : busy === "install" ? "installing" : busy === "activate" ? "activating" : "cancelling"]}</p>}
      {matchInProgress && <p className="pack-install-result">{text.matchActive}</p>}
      {error && <p className="pack-install-error" role="alert">{error}</p>}

      {summary && (
        <section aria-labelledby="pack-install-summary-title">
          <h3 id="pack-install-summary-title">{text.summary}</h3>
          <dl className="pack-install-details">
            <div><dt>{text.name}</dt><dd>{summary.display_name ?? text.unknownName}</dd></div>
            <div><dt>{text.revision}</dt><dd>{summary.revision ?? "—"}</dd></div>
            <div><dt>{text.events}</dt><dd>{summary.event_count ?? "—"}</dd></div>
            <div><dt>{text.bindings}</dt><dd>{summary.binding_count ?? "—"}</dd></div>
            <div><dt>{text.variants}</dt><dd>{summary.variant_count ?? "—"}</dd></div>
            <div><dt>{text.banners}</dt><dd>{summary.banner_count ?? "—"}</dd></div>
            <div><dt>{text.totalSize}</dt><dd>{summary.size_bytes === null ? "—" : formatPackageBytes(summary.size_bytes, locale)}</dd></div>
            <div><dt>{text.signature}</dt><dd>{summary.signature_state === "unavailable" ? text.signatureUnavailable : text.trustUnknown}</dd></div>
            <div><dt>{text.trust}</dt><dd>{summary.trust_state === "local_import" ? text.trustLocal : text.trustUnknown}</dd></div>
            <div><dt>{text.publisher}</dt><dd>{summary.publisher_claim === "none" ? text.publisherNone : summary.publisher_claim === "unverified" ? text.publisherUnverified : text.trustUnknown}</dd></div>
          </dl>
          {summary.status === "rejected" && summary.rejection_reason && (
            <p className="pack-install-rejection" role="alert">
              <strong>{text.rejection}:</strong> {reasonText(summary.rejection_reason)}
            </p>
          )}
          {summary.status === "valid" && <p className="pack-install-result">{text.ready}</p>}
        </section>
      )}

      {resultMessage && <p className="pack-install-result" role="status"><strong>{text.result}:</strong> {resultMessage}</p>}
      {cancelled && <p className="pack-install-result" role="status">{text.cancelled}</p>}

      <div className="pack-install-actions">
        <button
          type="button"
          className="pack-install-button primary"
          onClick={() => void install()}
          disabled={busy !== null || summary?.status !== "valid"}
        >
          {text.install}
        </button>
        {installResult?.status === "deferred" && !cancelled && (
          <>
            <span>{matchInProgress ? text.pendingDuringMatch : text.pendingElsewhere}</span>
            <button type="button" className="pack-install-button" onClick={() => void cancelPending()} disabled={busy !== null}>
              {text.cancelPending}
            </button>
          </>
        )}
      </div>
    </Dialog>
  );
}
