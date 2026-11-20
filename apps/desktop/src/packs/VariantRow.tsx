import type { EventVariantsCopy } from "./event-variants.copy";
import {
  auditionOutcomeText,
  variantRowPresentation,
  type EventVariant,
  type VariantAuditionResult,
} from "./variant-model";

export interface VariantRowProps {
  variant: EventVariant;
  copy: EventVariantsCopy;
  disabled: boolean;
  busy: boolean;
  outcome?: VariantAuditionResult;
  failureText?: string;
  onAudition: (variantId: string) => void;
}

export function VariantRow({
  variant,
  copy,
  disabled,
  busy,
  outcome,
  failureText,
  onAudition,
}: VariantRowProps) {
  const presentation = variantRowPresentation(variant, copy);

  return (
    <li className="kohbor-variant-row">
      <div className="kohbor-variant-row__identity">
        <strong>{copy.variantId} {variant.variant_id}</strong>
        {variant.clip_id && <span>{copy.clipId}: <code>{variant.clip_id}</code></span>}
      </div>
      <p className="kohbor-variant-row__caption">
        <span className="kohbor-variant-row__caption-label">{copy.caption}:</span> {presentation.captionText}
      </p>
      <div className="kohbor-variant-row__meta">
        <span>{copy.duration}: {presentation.durationText}</span>
        <span>{presentation.bannerText}</span>
      </div>
      <button
        type="button"
        className="kohbor-variant-row__play"
        disabled={disabled || busy}
        aria-busy={busy}
        aria-label={`${copy.play}: ${variant.variant_id}`}
        onClick={() => onAudition(variant.variant_id)}
      >
        {busy ? copy.playing : copy.play}
      </button>
      {outcome && (
        <p className="kohbor-variant-row__result" role="status">
          {auditionOutcomeText(outcome, copy)}
        </p>
      )}
      {failureText && <p className="kohbor-variant-row__result is-error" role="status">{failureText}</p>}
    </li>
  );
}
