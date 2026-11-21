import { useState } from "react";
import { auditionReason, packsPageCopy, type Locale } from "../copy";
import type { EventSound, SoundKey } from "./eventMap";
import { groupSoundRows, soundKey, soundLabel } from "./eventMap";
import { EventVariants } from "./EventVariants";
import { soundRowPresentation } from "./sound-row-model";
import type { VariantAuditionResult } from "./variant-model";

export interface ListenControl {
  block: (row: SoundKey) => string | null;
  play: (row: SoundKey) => void;
  busyKey: string | null;
}

export function ListenButton({ row, label, locale, control, inactive, forceDisabled, fallbackReason }: {
  row: SoundKey; label: string; locale: Locale; control: ListenControl; inactive?: boolean;
  forceDisabled?: boolean; fallbackReason?: string;
}) {
  const text = packsPageCopy(locale);
  const block = inactive ? text.inactiveListen : control.block(row);
  const reason = block && !inactive ? auditionReason(block, locale) : block;
  return <button type="button" className="packs-sound-play" disabled={forceDisabled || block !== null}
    title={reason ?? fallbackReason}
    aria-label={text.listenFor(label)} aria-busy={control.busyKey === soundKey(row)}
    onClick={() => control.play(row)}>
    <span className="packs-sound-play-icon" aria-hidden="true">
      {control.busyKey === soundKey(row) ? "…" : "▶"}
    </span>
  </button>;
}

export function SoundRows({ rows, locale, listen, disabled, onAuditionVariant }: {
  rows: EventSound[]; locale: Locale; listen: ListenControl; disabled: boolean;
  onAuditionVariant: (row: EventSound, variantId: string) => Promise<VariantAuditionResult>;
}) {
  const text = packsPageCopy(locale);
  const [expanded, setExpanded] = useState<string | null>(null);

  return <div className="packs-sound-groups">
    {groupSoundRows(rows).map(group => <section key={group.id} className="packs-sound-group">
      <h3>{text.groups[group.id]}</h3>
      <div className="packs-sound-card">
        <ul>
          {group.rows.map(row => {
            const label = soundLabel(row, locale);
            const key = soundKey(row);
            const variantsId = `event-variants-${key}`;
            const presentation = soundRowPresentation(row.state, row.variants.length);
            const block = listen.block(row);
            const description = block ? auditionReason(block, locale) : text.eventStates[row.state];
            const metadata = [
              row.state === "inherited" ? text.inheritedFrom(row.resolved_from_tier) : null,
              // The group label above the card already names the family; repeating it on every row is noise.
              label.detail === text.groups[group.id] ? null : label.detail,
              label.tier,
              row.variants.length > 0 ? text.voices(row.variants.length) : null,
              row.variants.length > 0 && row.selection ? text.rotation[row.selection] : null,
              row.variants.length > 0
                ? row.variants.some(variant => variant.has_visual) ? text.hasBanner : text.noBanner
                : null,
            ].filter(Boolean).join(" · ");
            const secondary = row.state === "ready" || row.state === "inherited"
              ? metadata
              : [text.eventStates[row.state], row.variants.length > 0 ? metadata : null].filter(Boolean).join(" · ");

            return <li className="packs-sound-row" key={key} data-state={row.state} title={description}
              aria-description={description}>
              <div className="packs-sound-main">
                {presentation.canExpand
                  ? <button type="button" className="packs-sound-expand"
                      aria-label={expanded === key ? text.hideVoices : text.showVoices}
                      aria-expanded={expanded === key} aria-controls={variantsId}
                      onClick={() => setExpanded(value => value === key ? null : key)}>
                      <span className="packs-sound-copy">
                        <strong className="packs-sound-name">{label.title}</strong>
                        {secondary && <span className={`packs-sound-description${row.state === "invalid_media" ? " is-warning" : ""}`}>
                          {row.state === "invalid_media" && <span aria-hidden="true">⚠ </span>}{secondary}
                        </span>}
                      </span>
                      <span className="packs-sound-chevron" aria-hidden="true">›</span>
                    </button>
                  : <div className="packs-sound-copy">
                      <strong className="packs-sound-name">{label.title}</strong>
                      {secondary && <span className={`packs-sound-description${row.state === "invalid_media" ? " is-warning" : ""}`}>
                        {row.state === "invalid_media" && <span aria-hidden="true">⚠ </span>}{secondary}
                      </span>}
                    </div>}
              </div>
              <div className="packs-sound-actions">
                {presentation.showPrimaryPlay && <ListenButton row={row} label={label.title} locale={locale} control={listen}
                  forceDisabled={presentation.disablePrimaryPlay} fallbackReason={text.eventStates[row.state]} />}
              </div>
              <div id={variantsId} className="packs-variants-expanded" hidden={expanded !== key}>
                {expanded === key && <EventVariants variants={row.variants} selection={row.selection} locale={locale}
                  disabled={disabled}
                  onAudition={variantId => onAuditionVariant(row, variantId)} />}
              </div>
            </li>;
          })}
        </ul>
      </div>
    </section>)}
  </div>;
}
