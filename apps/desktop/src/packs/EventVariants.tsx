import { useState } from "react";
import "./EventVariants.css";
import { eventVariantsCopy } from "./event-variants.copy";
import {
  auditionFailureText,
  variantSelectionLabel,
  type EventVariant,
  type VariantAuditionResult,
  type VariantSelection,
} from "./variant-model";
import { VariantRow } from "./VariantRow";

export interface EventVariantsProps {
  variants: readonly EventVariant[];
  selection: VariantSelection;
  locale: "th" | "en";
  disabled?: boolean;
  onAudition: (variantId: string) => Promise<VariantAuditionResult>;
}

export function EventVariants({
  variants,
  selection,
  locale,
  disabled = false,
  onAudition,
}: EventVariantsProps) {
  const copy = eventVariantsCopy(locale);
  const [busyVariantId, setBusyVariantId] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<{
    variantId: string;
    outcome?: VariantAuditionResult;
    failureText?: string;
  } | null>(null);

  const audition = async (variantId: string) => {
    setBusyVariantId(variantId);
    setLastResult(null);
    try {
      setLastResult({ variantId, outcome: await onAudition(variantId) });
    } catch (error) {
      setLastResult({ variantId, failureText: auditionFailureText(error, copy) });
    } finally {
      setBusyVariantId(null);
    }
  };

  const selectionText = variantSelectionLabel(selection, copy);
  const voiceNumbers = new Map(variants.map((variant, index) => [variant.variant_id, index + 1] as const));

  return (
    <section className="kohbor-event-variants" aria-label={copy.title}>
      {selectionText && <p className="kohbor-event-variants__selection">{copy.selectionLabel}: {selectionText}</p>}
      {variants.length === 0
        ? <p className="kohbor-event-variants__empty" role="status">{copy.noVariants}</p>
        : (
          <ul className="kohbor-event-variants__list">
            {variants.map((variant) => (
              <VariantRow
                key={variant.variant_id}
                variant={variant}
                voiceNumber={voiceNumbers.get(variant.variant_id)!}
                copy={copy}
                disabled={disabled || busyVariantId !== null}
                busy={busyVariantId === variant.variant_id}
                outcome={lastResult?.variantId === variant.variant_id ? lastResult.outcome : undefined}
                failureText={lastResult?.variantId === variant.variant_id ? lastResult.failureText : undefined}
                onAudition={(variantId) => void audition(variantId)}
              />
            ))}
          </ul>
        )}
    </section>
  );
}
