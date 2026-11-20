import { copy, packsPageCopy, type Locale } from "../copy";
import { outcomeTone } from "../history";
import { Chip, type Tone } from "../Readiness";
import { useAudioHistory } from "../useAudioHistory";
import { soundLabel } from "./eventMap";
import { sessionAudio } from "./model";

const TONE: Record<ReturnType<typeof outcomeTone>, Tone> = { played: "fresh", blocked: "warn", silent: "off" };

export function MatchRecap({ locale, disabled, epoch }: { locale: Locale; disabled: boolean; epoch: number | null }) {
  const text = packsPageCopy(locale);
  const { history, state, load } = useAudioHistory(disabled);
  const records = sessionAudio(history, epoch);
  return <>
    <div className="packs-section-heading">
      <h2>{text.recapTitle}</h2>
      <button type="button" disabled={disabled} onClick={() => void load()}>{text.retry}</button>
    </div>
    {(disabled || state === "unreadable" || state === "failed") && <p role="status">{text.recapFailed}</p>}
    {!disabled && state === "loading" && <p role="status">{copy(locale).history.loading}</p>}
    {state === "ok" && !disabled && records.length === 0 && <p className="packs-empty">{text.recapEmpty}</p>}
    {history && history.skipped > 0 && <p className="muted">{text.recapSkipped}</p>}
    {records.length > 0 && <ul className="packs-recap-list">
      {records.map(row => {
        const tone = outcomeTone(row.outcome);
        const label = row.outcome === "preempted" ? text.cut : text[tone];
        const detail = copy(locale).history.outcomes[row.outcome].detail;
        return <li className="packs-sound-row" key={row.instance_id}>
          <strong>{soundLabel({ event_id: row.event_type!, tier: null }, locale).title}</strong>
          <div><Chip tone={TONE[tone]} label={label} />
            {row.outcome !== "admitted" && <p className="muted">{detail}</p>}
          </div>
        </li>;
      })}
    </ul>}
    <p className="packs-footnote muted">{text.recapFootnote}</p>
  </>;
}
