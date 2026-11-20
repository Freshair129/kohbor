import { useId, useRef, useState } from "react";
import { copy, packReason, packsPageCopy, type Locale } from "../copy";
import { buildInstallRequest, errorCode, isPlausibleSourcePath, newRequestId, normalizeSourcePath, parseInspectResult, parseInstallResult } from "../packs";
import { mediaInspect, mediaInstall } from "../runtime";

export function PackInstall({ locale, disabled, onInstalled }: {
  locale: Locale;
  disabled: boolean;
  onInstalled: () => void;
}) {
  const text = copy(locale).packs;
  const page = packsPageCopy(locale);
  const pathId = useId();
  const [input, setInput] = useState("");
  const [checked, setChecked] = useState<{ path: string; digest: string; requestId: string } | null>(null);
  const [busy, setBusy] = useState<"inspect" | "install" | null>(null);
  const [notice, setNotice] = useState<{ text: string; error: boolean } | null>(null);
  const lock = useRef(false);

  const run = async (action: "inspect" | "install") => {
    if (lock.current || disabled) return;
    lock.current = true;
    setBusy(action);
    setNotice(null);
    try {
      const path = normalizeSourcePath(input);
      if (!isPlausibleSourcePath(path)) throw new Error("SOURCE_UNAVAILABLE");
      if (action === "inspect") {
        const result = parseInspectResult(await mediaInspect(path));
        if (!result) { setNotice({ text: text.inspectUnreadable, error: true }); return; }
        setChecked({ path, digest: result.container_sha256, requestId: newRequestId() });
      } else {
        if (!checked || checked.path !== path) return;
        const request = buildInstallRequest({
          request_id: checked.requestId, source_path: path, expected_container_sha256: checked.digest,
        });
        if (!request) { setNotice({ text: page.installFileChanged, error: true }); return; }
        const result = parseInstallResult(await mediaInstall(request));
        onInstalled();
        if (!result) { setNotice({ text: text.resultUnreadable, error: true }); return; }
        setChecked(null);
        if (result.status !== "rejected") setInput("");
        setNotice({
          text: result.reason ? `${page.outcomes[result.status]} · ${packReason(result.reason, locale)}` : page.outcomes[result.status],
          error: result.status === "rejected",
        });
      }
    } catch (error) {
      setNotice({ text: packReason(errorCode(error), locale), error: true });
    } finally {
      lock.current = false;
      setBusy(null);
    }
  };

  return <details className="packs-install">
    <summary>{page.installOpen}</summary>
    <div className="packs-install-form">
      <label htmlFor={pathId}>{text.pathLabel}</label>
      <p className="muted">{page.installHint}</p>
      <div className="packs-actions">
        <input id={pathId} type="text" value={input} spellCheck={false} autoComplete="off"
          disabled={disabled || busy !== null} onChange={event => {
            setInput(event.currentTarget.value);
            setChecked(null);
            setNotice(null);
          }} />
        <button type="button" disabled={disabled || busy !== null || !input.trim()}
          onClick={() => void run("inspect")}>{busy === "inspect" ? text.inspecting : text.inspect}</button>
      </div>
      {checked && <div className="packs-actions">
        <p role="status">{page.installVerified}</p>
        <button type="button" className="primary" disabled={disabled || busy !== null}
          onClick={() => void run("install")}>{busy === "install" ? text.installing : text.install}</button>
      </div>}
      {notice && <p className={notice.error ? "error" : undefined} role="status">{notice.text}</p>}
    </div>
  </details>;
}
