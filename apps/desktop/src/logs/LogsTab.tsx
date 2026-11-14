import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { logsCopy } from "./copy";
import { copyLogLines, filterLogEntries, formatLogBody, shouldFollowLogs, type LogFilter } from "./model";
import { openLogDirectory, readLogDirectory } from "./host";
import { useLogs } from "./useLogs";
import "./logs.css";

export function LogsTab({ visible, locale }: { visible: boolean; locale: "th" | "en" }) {
  const text = logsCopy(locale);
  const { state, tail } = useLogs(visible);
  const [level, setLevel] = useState<LogFilter>("all");
  const [query, setQuery] = useState("");
  const [copyCount, setCopyCount] = useState(50);
  const [following, setFollowing] = useState(true);
  const [directory, setDirectory] = useState<string | null>(null);
  const [notice, setNotice] = useState<"copied" | "manualCopy" | "openFailed" | null>(null);
  const [manualCopy, setManualCopy] = useState<string | null>(null);
  const list = useRef<HTMLOListElement | null>(null);
  const fallback = useRef<HTMLTextAreaElement | null>(null);
  const mounted = useRef(false);
  const rows = useMemo(() => filterLogEntries(state.entries, level, query), [state.entries, level, query]);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  useEffect(() => {
    if (!visible) return;
    let disposed = false;
    void readLogDirectory().then(path => { if (!disposed) setDirectory(path); });
    return () => { disposed = true; };
  }, [visible]);
  useLayoutEffect(() => {
    if (visible && following && list.current) list.current.scrollTop = list.current.scrollHeight;
  }, [rows, following, visible]);
  useLayoutEffect(() => {
    if (manualCopy === null) return;
    fallback.current?.focus();
    fallback.current?.select();
  }, [manualCopy]);

  const copy = async () => {
    const plain = copyLogLines(rows, copyCount);
    setManualCopy(null);
    try {
      await navigator.clipboard.writeText(plain);
      if (mounted.current) setNotice("copied");
    } catch {
      if (!mounted.current) return;
      // A selectable textarea also works when clipboard permission or the API is absent.
      setManualCopy(plain);
      setNotice("manualCopy");
    }
  };
  const openFolder = async () => {
    if (!await openLogDirectory() && mounted.current) setNotice("openFailed");
  };

  return (
    <section className="logs-tab" hidden={!visible} aria-label={text.title}>
      <div className="logs-toolbar">
        <label>{text.level}
          <select value={level} onChange={event => setLevel(event.currentTarget.value as LogFilter)}>
            <option value="all">{text.all}</option>
            {["error", "warn", "info", "debug"].map(value => <option key={value} value={value}>{value}</option>)}
          </select>
        </label>
        <label className="logs-filter">{text.filter}
          <input type="search" value={query} placeholder={text.filterPlaceholder} onChange={event => setQuery(event.currentTarget.value)} />
        </label>
        <button type="button" onClick={() => tail.setPaused(!state.paused)}>{state.paused ? text.resume : text.pause}</button>
        <button type="button" title={text.clearHint} onClick={() => { tail.clear(); setManualCopy(null); setNotice(null); }}>{text.clear}</button>
        <select aria-label={text.count} value={copyCount} onChange={event => setCopyCount(Number(event.currentTarget.value))}>
          {[50, 200, 500].map(count => <option key={count} value={count}>{count}</option>)}
        </select>
        <button type="button" title={text.copyHint} disabled={rows.length === 0} onClick={() => void copy()}>{text.copy(copyCount)}</button>
        <button type="button" onClick={() => void openFolder()}>{text.open}</button>
        {manualCopy !== null && <label className="logs-manual-copy">{text.manualLabel}
          <textarea ref={fallback} readOnly rows={1} value={manualCopy} onFocus={event => event.currentTarget.select()} />
        </label>}
      </div>
      <div className="logs-status">
        <span className="logs-folder" title={directory ?? undefined}>{text.folder}: <code>{directory ?? "—"}</code></span>
        <span>{text.rows(rows.length)}</span>
        <span>{state.paused ? text.paused : following ? text.following : text.stopped}</span>
        {!following && <button type="button" onClick={() => setFollowing(true)}>{text.follow}</button>}
        {state.dropped && <strong role="status">{text.dropped}</strong>}
        {state.status === "unavailable" && <span role="status">{text.unavailable}{rows.length > 0 ? ` · ${text.retained}` : ""}</span>}
        {notice && <span role="status">{text[notice]}</span>}
      </div>
      <ol className="logs-list" ref={list} aria-label={text.title} tabIndex={0}
        onScroll={event => setFollowing(shouldFollowLogs(event.currentTarget))}>
        {rows.map(entry => (
          <li className={`logs-row logs-${entry.level}`} key={entry.seq}>
            <span className="logs-time">{entry.ts}</span>
            <b className="logs-level">{entry.level}</b>
            <span className="logs-line">{formatLogBody(entry)}</span>
          </li>
        ))}
        {rows.length === 0 && <li className="logs-empty">{state.status === "unavailable" ? text.unavailable
          : state.status === "loading" ? text.loading : state.entries.length > 0 ? text.noMatch : text.empty}</li>}
      </ol>
    </section>
  );
}
