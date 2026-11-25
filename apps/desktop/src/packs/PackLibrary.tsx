import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from "react";
import { packsPageCopy, type Locale } from "../copy";
import { packCoverWebviewUrl } from "../mediaUrl";
import { formatBytes } from "../packs";
import { Chip } from "../Readiness";
import { BROWSE_EVENT_CATEGORIES, browseCategoryRows, soundLabel, soundKey, type BrowseEventCategory } from "./eventMap";
import type { BrowseBinding, BrowsePackDetail } from "./browseApi";
import type { BrowsePreviewState } from "./previewModel";
import type { PackSummary } from "./model";
import { browseVoiceLabel, type VariantAuditionResult } from "./variant-model";
import { eventVariantsCopy } from "./event-variants.copy";

const PLATFORM = navigator.userAgent.includes("Windows") ? "windows" : "other";

function originLabel(pack: PackSummary, locale: Locale) {
  if (pack.authorityClass === "system") return { label: locale === "th" ? "ทางการ" : "Official", className: "packs-origin-chip packs-origin-chip--official" };
  if (pack.authorityClass === "local_import") return { label: locale === "th" ? "กำหนดเอง · จากไฟล์" : "Custom · imported from file", className: "packs-origin-chip packs-origin-chip--custom" };
  return { label: locale === "th" ? "ที่มา: ยังแยกไม่ได้" : "Source: not yet distinguishable", className: "packs-origin-chip" };
}

function PackCover({ logicalUrl, initial, reportFailure }: {
  logicalUrl: string | null; initial: string; reportFailure: () => void;
}) {
  const src = logicalUrl && packCoverWebviewUrl(logicalUrl, PLATFORM);
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
    {src && visible && !failed && <img className={ready ? "is-ready" : undefined} src={src} alt=""
      decoding="async" draggable={false} onLoad={() => setReady(true)}
      onError={() => { setFailed(true); reportFailure(); }} />}
  </div>;
}

function categoryCoverage(pack: PackSummary, category: BrowseEventCategory) {
  const ids = new Set(browseCategoryRows(category).map(row => soundKey(row)));
  return pack.bindings.reduce((total, binding) => ids.has(soundKey(binding)) ? total + binding.variantCount : total, 0);
}

export function PackCard({ pack, locale, unavailable, busy, activationError, selected, onActivate, onDetails }: {
  pack: PackSummary; locale: Locale; unavailable: boolean; busy: boolean; activationError?: string; selected?: boolean;
  onActivate: () => void; onDetails: (button: HTMLButtonElement) => void;
}) {
  const id = useId();
  const text = packsPageCopy(locale);
  const [failedCover, setFailedCover] = useState<string>();
  const coverFailed = pack.coverUrl !== null && failedCover === pack.coverUrl;
  const problem = activationError ?? (pack.problem ? text.problems[pack.problem] : null) ?? (coverFailed ? text.problems.cover : null);
  const equipped = pack.equippedSlot !== null;
  const state = problem ? "problem" : pack.active ? "active" : "installed";
  const origin = originLabel(pack, locale);
  return <article className={`packs-card${pack.active ? " is-active" : ""}${selected ? " is-open" : ""}`} aria-labelledby={id}>
    <button type="button" className="packs-card-open" aria-haspopup="dialog" aria-label={text.detailsFor(pack.name ?? text.unnamed)}
      onClick={event => onDetails(event.currentTarget)}>
      <PackCover key={pack.coverUrl} logicalUrl={pack.coverUrl} initial={Array.from(pack.name ?? "")[0] ?? "—"}
        reportFailure={() => setFailedCover(pack.coverUrl ?? undefined)} />
      <div className="packs-card-body">
        <h2 id={id} dir="auto">{pack.name ?? text.unnamed}</h2>
        <div className="packs-card-meta">
          <span className={origin.className}>{origin.label}</span>
          <Chip tone={state === "problem" ? "warn" : state === "active" ? "fresh" : "off"} label={text.states[state]} />
          <Chip tone={equipped ? "fresh" : "off"} label={equipped
            ? (locale === "th" ? `ช่อง ${pack.equippedSlot! + 1}` : `Equipped · slot ${pack.equippedSlot! + 1}`)
            : (locale === "th" ? (pack.packKind === "special" ? "แพ็กเหตุการณ์พิเศษ" : pack.packKind === "mixed" ? "พื้นฐานและพิเศษ" : "แพ็กพื้นฐาน")
              : (pack.packKind === "special" ? "Special" : pack.packKind === "mixed" ? "Mixed" : pack.packKind === "basic" ? "Basic" : "Kind unknown"))} />
          {state === "problem" && pack.active && <Chip tone="fresh" label={text.states.active} />}
          <small className="muted">{text.revision(pack.revision)}</small>
        </div>
        <p>{pack.counts ? text.summary(pack.counts.events, pack.counts.voices, pack.counts.banners) : text.summaryUnknown}</p>
        {problem && <p className="packs-problem">{problem}</p>}
        {pack.pending && <p className="muted">{text.pending}</p>}
        <div className="packs-card-pips" aria-label={locale === "th" ? "เสียงในห้าหมวด" : "Voice coverage in five categories"}>
          {BROWSE_EVENT_CATEGORIES.map(category => {
            const count = categoryCoverage(pack, category);
            return <span key={category} className={`packs-card-pip${count > 0 ? " is-covered" : ""}`}
              title={`${text.groups[category]}: ${count}`} aria-label={`${text.groups[category]}: ${count}`}>
              {text.groups[category].slice(0, 1)}
            </span>;
          })}
        </div>
      </div>
    </button>
    <div className="packs-actions packs-card-actions">
      {pack.pending
          ? <button type="button" disabled>{text.pending}</button>
          : <button type="button" className="primary" disabled={unavailable || pack.problem === "metadata"}
            aria-busy={busy} aria-label={equipped
              ? (locale === "th" ? `ถอดแพ็ก ${pack.name ?? text.unnamed}` : `Unequip ${pack.name ?? text.unnamed}`)
              : (locale === "th" ? `ติดตั้งเป็นแพ็กเสียง ${pack.name ?? text.unnamed}` : `Equip ${pack.name ?? text.unnamed}`)} onClick={onActivate}>
            {busy ? text.activating : equipped ? (locale === "th" ? "ถอดออก" : "Unequip") : (locale === "th" ? "ติดตั้งเพิ่ม" : "Equip")}
          </button>}
      <button type="button" aria-haspopup="dialog" aria-label={text.detailsFor(pack.name ?? text.unnamed)}
        onClick={event => onDetails(event.currentTarget)}>{text.details}</button>
    </div>
  </article>;
}

function factValue(value: string | null, fallback: string) {
  return value?.trim() ? value : fallback;
}

function canPreviewBinding(binding: BrowseBinding) {
  return binding.variants.length > 0 && (binding.state === "ready" || binding.state === "inherited"
    || (binding.state === "detector_unavailable" && binding.reason === "EVENT_UNAVAILABLE"));
}

export function PackDrawer({ pack, packs, detail, loading, locale, preview, previewNotice, matchLocked,
  overlayPreview, onOverlayPreview, onClose, returnFocus, onStep, onEquip, actionBusy,
  onAuditionVariant, onPlayCategory, previewVideo, onPreviewVisualError,
}: {
  pack: PackSummary; packs: readonly PackSummary[]; detail: BrowsePackDetail | null; loading: boolean;
  locale: Locale; preview: BrowsePreviewState; previewNotice: string | null; matchLocked: boolean; overlayPreview: boolean;
  onOverlayPreview: (value: boolean) => void; onClose: () => void; returnFocus: HTMLButtonElement;
  onStep: (pack: PackSummary) => void; onEquip: () => void; actionBusy: boolean;
  onAuditionVariant: (binding: BrowseBinding, variantId: string) => Promise<VariantAuditionResult>;
  onPlayCategory: (bindings: readonly BrowseBinding[]) => void;
  previewVideo: RefObject<HTMLVideoElement | null>;
  onPreviewVisualError: (playbackId: string, reason: string) => void;
}) {
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const close = useRef<HTMLButtonElement>(null);
  const text = packsPageCopy(locale);
  const voiceText = eventVariantsCopy(locale);
  const [category, setCategory] = useState<BrowseEventCategory>("kill");
  const packIndex = packs.findIndex(item => item.installId === pack.installId);
  const previewId = preview.installId === pack.installId ? preview.playbackId : null;
  const showVideo = previewId !== null && preview.visualUrl !== null;
  const playing = previewId !== null && preview.playing;
  const origin = originLabel(pack, locale);
  const [coverFailed, setCoverFailed] = useState(false);

  useLayoutEffect(() => {
    const node = dialog.current!;
    node.showModal();
    close.current?.focus();
    return () => {
      node.close();
      if (returnFocus.isConnected) returnFocus.focus();
    };
  }, [returnFocus]);

  const rows = browseCategoryRows(category);
  const detailByKey = new Map(detail?.bindings.map(binding => [soundKey(binding), binding]) ?? []);
  const playableRows = rows.flatMap(row => {
    const binding = detailByKey.get(soundKey(row));
    return binding && canPreviewBinding(binding) ? [binding] : [];
  });
  const categoryVoiceCount = playableRows.reduce((sum, row) => sum + row.variants.length, 0);
  const technicalRows = detail?.bindings.filter(binding => binding.state !== "inherited" && binding.variants.length > 0) ?? [];

  return <dialog ref={dialog} className="packs-drawer glass--overlay glass-sheet" aria-labelledby={id} onCancel={event => {
    event.preventDefault();
    onClose();
  }} onKeyDown={event => {
    if (event.altKey && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      const next = packs[packIndex + (event.key === "ArrowLeft" ? -1 : 1)];
      if (next) onStep(next);
      return;
    }
    if (event.target instanceof HTMLButtonElement && event.target.getAttribute("role") === "tab"
      && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      event.preventDefault();
      const index = BROWSE_EVENT_CATEGORIES.indexOf(category);
      const next = BROWSE_EVENT_CATEGORIES[(index + (event.key === "ArrowRight" ? 1 : -1) + BROWSE_EVENT_CATEGORIES.length) % BROWSE_EVENT_CATEGORIES.length]!;
      setCategory(next);
      dialog.current?.querySelector<HTMLButtonElement>(`[data-category="${next}"]`)?.focus();
      return;
    }
    if (event.key !== "Tab") return;
    const targets = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), summary, [tabindex='0']"));
    const first = targets[0], last = targets.at(-1);
    if (event.shiftKey && document.activeElement === first && last) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last && first) { event.preventDefault(); first.focus(); }
  }}>
    <div className="packs-drawer-content">
      <header className="packs-section-heading packs-drawer-title">
        <div><h2 id={id} dir="auto">{pack.name ?? text.unnamed}</h2><p className="muted">{text.revision(pack.revision)}</p></div>
        <div>
          <button type="button" aria-label={text.previousPack} disabled={packIndex <= 0}
            onClick={() => { const next = packs[packIndex - 1]; if (next) onStep(next); }}>‹</button>
          <span className="muted" aria-live="polite">{packIndex < 0 ? "—" : `${packIndex + 1}/${packs.length}`}</span>
          <button type="button" aria-label={text.nextPack} disabled={packIndex < 0 || packIndex >= packs.length - 1}
            onClick={() => { const next = packs[packIndex + 1]; if (next) onStep(next); }}>›</button>
          <button ref={close} type="button" onClick={onClose}>{text.close}</button>
        </div>
      </header>

      <section className="packs-preview-stage glass-solid" aria-label={locale === "th" ? "ตัวอย่างเสียงและแบนเนอร์" : "Sound and banner preview"}>
        <div className="packs-preview-frame">
          {!previewId && pack.coverUrl && !coverFailed && <img className="packs-preview-poster" src={packCoverWebviewUrl(pack.coverUrl, PLATFORM) ?? undefined}
            alt="" onError={() => setCoverFailed(true)} />}
          <video ref={previewVideo} className={showVideo ? undefined : "packs-preview-video-hidden"}
            muted playsInline preload="auto" aria-hidden="true"
            onError={() => { if (previewId && showVideo) onPreviewVisualError(previewId, "VIDEO_ERROR"); }} />
          {!previewId && (!pack.coverUrl || coverFailed) && <p className="packs-preview-empty">{text.previewIdle}</p>}
          {previewId && !preview.hasVisual && <p className="packs-preview-empty">{text.previewAudioOnly}</p>}
          {previewId && preview.visualFailure && <p className="packs-preview-empty" role="status">{text.previewFailed}</p>}
        </div>
        <span key={previewId ?? "idle"} className={`packs-preview-progress${playing ? " is-playing" : ""}`}
          style={{ "--preview-duration": `${Math.max(1, preview.durationMs ?? 0)}ms` } as CSSProperties} aria-hidden="true"><span /></span>
        <p className="packs-preview-status" role="status">{previewNotice ?? (loading ? text.previewLoading : "")}</p>
        <div className="packs-preview-controls">
          <label><input type="checkbox" checked={overlayPreview} disabled={!equipped || matchLocked}
            onChange={event => onOverlayPreview(event.currentTarget.checked)} />{text.overlayOption}</label>
          <small>{matchLocked ? text.matchLock : equipped ? "" : text.inactiveListen}</small>
        </div>
      </section>

      <div className="packs-drawer-tabs" role="tablist" aria-label={locale === "th" ? "หมวดเหตุการณ์" : "Event categories"}>
        {BROWSE_EVENT_CATEGORIES.map(name => <button key={name} type="button" role="tab" data-category={name}
          aria-selected={category === name} aria-controls={`${id}-events`} tabIndex={category === name ? 0 : -1}
          onClick={() => setCategory(name)}>{text.groups[name]} <span>{pack.bindings.filter(binding => browseCategoryRows(name).some(row => soundKey(row) === soundKey(binding))).reduce((n, binding) => n + binding.variantCount, 0)}</span>
        </button>)}
      </div>
      <div className="packs-drawer-scroll glass-solid" tabIndex={0}>
        <div className="packs-drawer-info"><span className={origin.className}>{origin.label}</span>
          <Chip tone={equipped ? "fresh" : "off"} label={equipped
            ? (locale === "th" ? `ติดตั้งในช่อง ${pack.equippedSlot! + 1}` : `Equipped · slot ${pack.equippedSlot! + 1}`)
            : text.states.installed} />
          {pack.pending && <Chip tone="warn" label={text.pending} />}
          {pack.problem && <Chip tone="warn" label={text.problems[pack.problem]} />}
        </div>
        <details className="packs-facts-wrap">
          <summary>{locale === "th" ? "เกี่ยวกับแพ็กนี้" : "About this pack"}</summary>
          <p dir="auto">{factValue(pack.presentation.description, text.noPackField)}</p>
          <dl className="packs-facts">
            <dt>{locale === "th" ? "ผู้จัดทำ" : "Author"}</dt><dd>{factValue(pack.presentation.author, text.noPackField)}{pack.presentation.author && <span className="muted"> · {locale === "th" ? "ตามที่แพ็กระบุ ยังไม่ได้ยืนยัน" : "claimed by the pack, not verified"}</span>}</dd>
            <dt>{locale === "th" ? "ภาษาเสียง" : "Languages"}</dt><dd>{pack.presentation.languages?.length ? pack.presentation.languages.join(", ") : text.noPackField}</dd>
            {pack.publisherClaim && <><dt>{locale === "th" ? "คำอ้างผู้เผยแพร่" : "Publisher claim"}</dt><dd>{text.publisherClaim(pack.publisherClaim)}</dd></>}
            <dt>{text.installedAt}</dt><dd>{pack.installedAt ?? "—"}</dd>
            <dt>{text.sizeLabel}</dt><dd>{pack.sizeBytes === null ? "—" : formatBytes(pack.sizeBytes)}</dd>
            <dt>{locale === "th" ? "เกม" : "Game"}</dt><dd>{pack.gameId === "dota2" ? "Dota 2" : "—"}</dd>
          </dl>
        </details>
        <div className="packs-event-list" id={`${id}-events`} role="tabpanel">
          <div className="packs-section-heading">
            <h3>{text.groups[category]}</h3><span className="muted">{categoryVoiceCount} {text.voices(categoryVoiceCount)}</span>
            <button type="button" disabled={categoryVoiceCount === 0 || loading || matchLocked || actionBusy}
              onClick={() => onPlayCategory(playableRows)}>{text.categorySampler}</button>
          </div>
          {loading && <p role="status">{text.previewLoading}</p>}
          {!loading && !detail && <p role="status">{text.unsupportedPreview}</p>}
          <div className="packs-event-rows">
            {rows.map(row => {
              const key = soundKey(row);
              const summaryBinding = pack.bindings.find(binding => soundKey(binding) === key);
              const binding = detailByKey.get(key);
              const label = soundLabel(row, locale);
              if ((!binding && !summaryBinding) || binding?.state === "unmapped") return <div className="packs-event-row packs-event-row--missing" key={key}>
                <strong>{label.title}</strong>{label.tier && <span className="muted"> · {label.tier}</span>}
                <p>{text.eventMissing}</p>
              </div>;
              if (!binding) return <div className="packs-event-row" key={key}><strong>{label.title}</strong><p>{text.eventUnknown}</p></div>;
              if (binding.state === "detector_unavailable" && !canPreviewBinding(binding)) return <div className="packs-event-row packs-event-row--missing" key={key}>
                <strong>{label.title}</strong><p>{text.detectorUnavailable}</p>
              </div>;
              if (binding.state === "not_voiced") return <div className="packs-event-row packs-event-row--missing" key={key}>
                <strong>{label.title}</strong><p>{text.eventMissing}</p>
              </div>;
              if (!canPreviewBinding(binding)) return <div className="packs-event-row" key={key}>
                <strong>{label.title}</strong><p>{text.eventUnknown}</p>
              </div>;
              return <section className="packs-event-row" key={key}>
                <div><strong>{label.title}</strong>{label.tier && <span className="muted"> · {label.tier}</span>}
                  {binding.state === "inherited" && <p className="muted">{text.inheritedFrom(binding.resolved_from_tier)}</p>}</div>
                <ul className="packs-voice-list">
                  {binding.variants.map((variant, index) => {
                    const labelText = browseVoiceLabel(variant, [label.title, label.tier].filter(Boolean).join(" · "), index, locale);
                    const isPlaying = playing && preview.soundKey === key && preview.variantId === variant.variant_id;
                    return <li className="packs-voice-row" data-playing={isPlaying} key={variant.variant_id}>
                      <div>
                        <strong className="packs-voice-caption" dir="auto">{labelText}</strong>
                        <span className="packs-voice-meta">
                          {(variant.duration_ms / 1000).toFixed(1)} {voiceText.secondsUnit}
                          {variant.has_visual && ` · ${voiceText.bannerYes}`}
                          {isPlaying && ` · ${voiceText.playing}`}
                        </span>
                      </div>
                      <button type="button" aria-label={`${voiceText.play} ${labelText}`} disabled={matchLocked || actionBusy}
                        onClick={() => { void onAuditionVariant(binding, variant.variant_id).catch(() => {}); }}>
                        {isPlaying ? voiceText.playing : voiceText.play}
                      </button>
                    </li>;
                  })}
                </ul>
              </section>;
            })}
          </div>
        </div>
        {technicalRows.length > 0 && <details className="packs-facts-wrap packs-technical">
          <summary>{locale === "th" ? "ข้อมูลทางเทคนิค" : "Technical details"}</summary>
          <ul>{technicalRows.flatMap(binding => {
            const event = soundLabel(binding, locale);
            return binding.variants.map((variant, index) => <li key={`${soundKey(binding)}:${variant.variant_id}`}>
              {browseVoiceLabel(variant, [event.title, event.tier].filter(Boolean).join(" · "), index, locale)}
              {" · "}{voiceText.variantId} <code>{variant.variant_id}</code>
              {variant.clip_id && <>{" · "}{voiceText.clipId} <code>{variant.clip_id}</code></>}
            </li>);
          })}</ul>
        </details>}
      </div>

      <footer className="packs-drawer-footer">
        <span>{matchLocked ? text.matchLock : pack.pending ? text.pending : equipped ? (locale === "th" ? `กำลังใช้ในช่อง ${pack.equippedSlot! + 1}` : `Equipped in slot ${pack.equippedSlot! + 1}`) : (locale === "th" ? "เปลี่ยนแล้วมีผลหลังจบแมตช์" : "Changes apply after the current match")}</span>
        <span className="sp" />
        {pack.pending
          ? <button type="button" disabled>{text.pending}</button>
          : <button type="button" className="primary packs-equip-button" disabled={actionBusy || pack.problem === "metadata"} onClick={onEquip}>
            {actionBusy ? text.activating : equipped ? (locale === "th" ? "ถอดออก" : "Unequip") : (locale === "th" ? "ติดตั้งเพิ่ม" : "Equip")}
          </button>}
      </footer>
    </div>
  </dialog>;
}
