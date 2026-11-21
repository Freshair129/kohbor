import type { EventVariantsCopy } from "./event-variants.copy";
import {
  auditionOutcomeText,
  variantRowPresentation,
  type EventVariant,
  type VariantAuditionResult,
} from "./variant-model";

export interface VariantRowProps {
  variant: EventVariant;
  voiceNumber: number;
  copy: EventVariantsCopy;
  disabled: boolean;
  busy: boolean;
  outcome?: VariantAuditionResult;
  failureText?: string;
  onAudition: (variantId: string) => void;
}

export function VariantRow({
  variant,
  voiceNumber,
  copy,
  disabled,
  busy,
  outcome,
  failureText,
  onAudition,
}: VariantRowProps) {
  const presentation = variantRowPresentation(variant, copy);
  const identity = variant.clip_id
    ? `${copy.variantId}: ${variant.variant_id} · ${copy.clipId}: ${variant.clip_id}`
    : `${copy.variantId}: ${variant.variant_id}`;

  return (
    <li className="kohbor-variant-row" title={identity} data-banner-state={presentation.bannerText}
      data-variant-id={variant.variant_id} data-clip-id={variant.clip_id ?? undefined}>
      <div className="kohbor-variant-row__voice">
        <strong>{copy.voice(voiceNumber)}</strong>
        {variant.caption && <p className="kohbor-variant-row__caption">{presentation.captionText}</p>}
      </div>
      <div className="kohbor-variant-row__actions">
        <span className="kohbor-variant-row__duration">{presentation.durationText}</span>
        <button
          type="button"
          className="kohbor-variant-row__play"
          disabled={disabled || busy}
          aria-busy={busy}
          aria-label={`${copy.play}: ${copy.voice(voiceNumber)}`}
          title={identity}
          onClick={() => onAudition(variant.variant_id)}
        >
          <span className="kohbor-variant-row__play-icon" aria-hidden="true">{busy ? "…" : "▶"}</span>
        </button>
      </div>
      {outcome && (
        <p className="kohbor-variant-row__result" role="status">
          {auditionOutcomeText(outcome, copy)}
        </p>
      )}
      {failureText && <p className="kohbor-variant-row__result is-error" role="status">{failureText}</p>}
    </li>
  );
}
