import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { packsPageCopy, type Locale } from "../copy";
import { packCoverWebviewUrl } from "../mediaUrl";
import { Chip } from "../Readiness";
import { groupSoundRows, soundKey } from "./eventMap";
import { cardView, packBindingView, type PackSummary } from "./model";
import { ListenButton, type ListenControl } from "./SoundRows";

function PackCover({ logicalUrl, initial, reportFailure }: {
  logicalUrl: string | null; initial: string; reportFailure: () => void;
}) {
  const platform = navigator.userAgent.includes("Windows") ? "windows" : "other";
  const src = logicalUrl && packCoverWebviewUrl(logicalUrl, platform);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const [visible, setVisible] = useState(false);
  const region = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!src || !region.current) return;
    if (typeof IntersectionObserver === "undefined") { setVisible(true); return; }
    const observer = new IntersectionObserver(entries => {
      if (!entries.some(entry => entry.isIntersecting)) return;
      setVisible(true);
      observer.disconnect();
    });
    observer.observe(region.current);
    return () => observer.disconnect();
  }, [src]);
  return <div ref={region} className="packs-cover" aria-hidden="true">
    <span>{initial}</span>
    {src && visible && !failed && <video className={ready ? "is-ready" : undefined} src={src}
      muted playsInline preload="auto" tabIndex={-1}
      onLoadedMetadata={({ currentTarget: video }) => {
        if (Number.isFinite(video.duration) && video.duration > 0) video.currentTime = Math.min(0.35, video.duration / 4);
      }}
      onLoadedData={({ currentTarget: video }) => { if (!video.seeking) setReady(true); }} onSeeked={() => setReady(true)}
      onError={() => { setFailed(true); reportFailure(); }} />}
  </div>;
}

export function PackCard({ pack, locale, unavailable, busy, activationError, onActivate, onDetails }: {
  pack: PackSummary; locale: Locale; unavailable: boolean; busy: boolean; activationError?: string;
  onActivate: () => void; onDetails: (button: HTMLButtonElement) => void;
}) {
  const id = useId();
  const text = packsPageCopy(locale);
  const view = cardView(pack, locale);
  const [failedCover, setFailedCover] = useState<string>();
  const coverFailed = view.coverUrl !== null && failedCover === view.coverUrl;
  const problem = activationError ?? view.reason ?? (coverFailed ? text.problems.cover : null);
  const status = problem ? "problem" : view.status;
  return <article className="packs-card" aria-labelledby={id}>
    <PackCover key={view.coverUrl} logicalUrl={view.coverUrl} initial={view.initial}
      reportFailure={() => setFailedCover(view.coverUrl ?? undefined)} />
    <div className="packs-card-body">
      <h2 id={id} dir="auto">{view.name}</h2>
      <div className="packs-card-meta">
        <Chip tone={status === "problem" ? "warn" : status === "active" ? "fresh" : "off"} label={text.states[status]} />
        {status === "problem" && pack.active && <Chip tone="fresh" label={text.states.active} />}
        <small className="muted">{view.revision}</small>
      </div>
      <p>{view.summary}</p>
      {problem && <p className="packs-problem">{problem}</p>}
      {pack.pending && <p className="muted">{text.pending}</p>}
    </div>
    <div className="packs-actions packs-card-actions">
      {!pack.active && !pack.pending && <button type="button" className="primary" disabled={unavailable}
        aria-busy={busy} aria-label={text.activatePack(view.name)} onClick={onActivate}>
        {busy ? text.activating : text.activate}
      </button>}
      <button type="button" aria-haspopup="dialog" aria-label={text.detailsFor(view.name)}
        onClick={event => onDetails(event.currentTarget)}>{text.details}</button>
    </div>
  </article>;
}

export function PackDrawer({ pack, locale, listen, onClose, returnFocus, auditionNotice, matchLocked }: {
  pack: PackSummary; locale: Locale; listen: ListenControl; onClose: () => void;
  returnFocus: HTMLButtonElement; auditionNotice: string | null; matchLocked: boolean;
}) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const text = packsPageCopy(locale);
  const view = cardView(pack, locale);
  const [failedCover, setFailedCover] = useState<string>();
  const coverFailed = view.coverUrl !== null && failedCover === view.coverUrl;

  useLayoutEffect(() => {
    const node = dialog.current!;
    node.showModal();
    close.current?.focus();
    return () => {
      node.close();
      if (returnFocus.isConnected) returnFocus.focus();
    };
  }, [returnFocus]);

  return <dialog ref={dialog} className="packs-drawer" aria-labelledby={id} onCancel={event => {
    event.preventDefault();
    onClose();
  }} onKeyDown={event => {
    if (event.key !== "Tab") return;
    const targets = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled), [tabindex='0']"));
    const first = targets[0], last = targets.at(-1);
    if (event.shiftKey && document.activeElement === first && last) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last && first) {
      event.preventDefault();
      first.focus();
    }
  }}>
    <div className="packs-drawer-content">
      <header className="packs-section-heading">
        <div><h2 id={id} dir="auto">{view.name}</h2><p className="muted">{view.revision}</p></div>
        <button ref={close} type="button" onClick={onClose}>{text.close}</button>
      </header>
      <div className="packs-drawer-scroll" tabIndex={0}>
        <PackCover key={view.coverUrl} logicalUrl={view.coverUrl} initial={view.initial}
          reportFailure={() => setFailedCover(view.coverUrl ?? undefined)} />
        {(view.reason || coverFailed) && <p role="status">{view.reason ?? text.problems.cover}</p>}
        {!pack.active && <p className="muted">{text.inactiveListen}</p>}
        {pack.active && matchLocked && <p className="muted">{text.matchLock}</p>}
        {pack.active && <p role="status" className="packs-audition-result">{auditionNotice}</p>}
        {pack.counts && pack.bindings.length === 0 && <p>{text.noneCovered}</p>}
        {groupSoundRows(pack.bindings).map(group => <section className="packs-sound-group" key={group.id}>
          <h3>{text.groups[group.id]}</h3>
          <ul>
            {group.rows.map(row => {
              const label = packBindingView(row, locale);
              return <li className="packs-sound-row" key={soundKey(row)}>
                <div><strong>{label.title}</strong>
                  {(label.detail || label.tier) && <p className="muted">{[label.detail, label.tier].filter(Boolean).join(" · ")}</p>}
                </div>
                <p className="muted">{[label.voices, label.rotation, label.banners].filter(Boolean).join(" · ")}</p>
                <ListenButton row={row} locale={locale} label={label.title} control={listen} inactive={!pack.active} />
              </li>;
            })}
          </ul>
        </section>)}
      </div>
    </div>
  </dialog>;
}
