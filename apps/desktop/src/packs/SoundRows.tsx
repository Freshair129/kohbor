import { auditionReason, packsPageCopy, type Locale } from "../copy";
import { Chip, type Tone } from "../Readiness";
import type { EventSound, EventSoundState, SoundKey } from "./eventMap";
import { groupSoundRows, soundKey, soundLabel } from "./eventMap";

export interface ListenControl {
  block: (row: SoundKey) => string | null;
  play: (row: SoundKey) => void;
  busyKey: string | null;
}

const EVENT_TONES: Record<EventSoundState, Tone> = {
  ready: "fresh", inherited: "fresh", unmapped: "off", invalid_media: "warn",
  detector_unavailable: "off", not_voiced: "off", unknown: "unknown",
};

export function ListenButton({ row, label, locale, control, inactive }: {
  row: SoundKey; label: string; locale: Locale; control: ListenControl; inactive?: boolean;
}) {
  const text = packsPageCopy(locale);
  const block = inactive ? text.inactiveListen : control.block(row);
  const reason = block && !inactive ? auditionReason(block, locale) : block;
  return <button type="button" disabled={block !== null} title={reason ?? undefined}
    aria-label={text.listenFor(label)} aria-busy={control.busyKey === soundKey(row)}
    onClick={() => control.play(row)}>{text.listen}</button>;
}

export function SoundRows({ rows, locale, listen }: { rows: EventSound[]; locale: Locale; listen: ListenControl }) {
  const text = packsPageCopy(locale);
  return <div className="packs-sound-groups">
    {groupSoundRows(rows).map(group => <section key={group.id} className="packs-sound-group">
      <h3>{text.groups[group.id]}</h3>
      <ul>
        {group.rows.map(row => {
          const label = soundLabel(row, locale);
          return <li className="packs-sound-row" key={soundKey(row)}>
            <div><strong>{label.title}</strong>
              {(label.detail || label.tier) && <p className="muted">{[label.detail, label.tier].filter(Boolean).join(" · ")}</p>}
            </div>
            <div>
              <Chip tone={EVENT_TONES[row.state]} label={text.eventStates[row.state]} />
              {row.state === "inherited" && <p className="muted">{text.inheritedFrom(row.resolved_from_tier)}</p>}
              {row.variants.length > 0 && <p className="muted">
                {[text.voices(row.variants.length), row.selection && text.rotation[row.selection],
                  row.variants.some(variant => variant.has_visual) ? text.hasBanner : text.noBanner].filter(Boolean).join(" · ")}
              </p>}
            </div>
            <ListenButton row={row} label={label.title} locale={locale} control={listen} />
          </li>;
        })}
      </ul>
    </section>)}
  </div>;
}
