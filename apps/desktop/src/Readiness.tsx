import type { ReactElement } from "react";
import { copy, mediaReason, reason, seconds, type Locale } from "./copy";
import type { RuntimeStatus } from "./types";

/**
 * Honest-state encodings (contracts/design-tokens.json): each tone differs by
 * glyph, border style and words, never by colour alone.
 */
export type Tone = "fresh" | "stale" | "unknown" | "off" | "warn";

export function Chip({ tone, label }: { tone: Tone; label: string }) {
  return (
    <span className={`chip chip-${tone}`}>
      <span className="chip-glyph" aria-hidden="true" />
      {label}
    </span>
  );
}

function Row({ name, chip, detail }: { name: string; chip: ReactElement; detail: string }) {
  return (
    <div className="row" role="row">
      <span className="row-name" role="rowheader">
        {name}
      </span>
      <span role="cell">{chip}</span>
      <span className="row-detail" role="cell">
        {detail}
      </span>
    </div>
  );
}

function megabytes(bytes: number | null): string {
  return bytes === null ? "—" : String(Math.round(bytes / 1e6));
}

export function Readiness({ status, locale }: { status: RuntimeStatus; locale: Locale }) {
  const text = copy(locale).readiness;
  const phases = copy(locale).phases;
  const i = status.intake;
  const receiving = i.state === "receiving";

  const intakeChip: Record<RuntimeStatus["intake"]["state"], ReactElement> = {
    not_configured: <Chip tone="unknown" label={text.intake.not_configured} />,
    starting: <Chip tone="unknown" label={text.intake.starting} />,
    listening: <Chip tone="unknown" label={text.intake.listening} />,
    receiving: <Chip tone="fresh" label={text.intake.receiving} />,
    stale: <Chip tone="stale" label={text.intake.stale} />,
    unavailable: <Chip tone="warn" label={text.intake.unavailable} />,
  };
  const intakeDetail =
    i.state === "receiving" || i.state === "stale"
      ? text.lastPortAge(i.port, seconds(i.last_admitted_age_ms, locale))
      : i.state === "listening"
        ? text.portWaiting(i.port)
        : reason(i.reason, locale);

  // Ownership and match describe the last admitted payload. Once the source is
  // not live they are shown as old, never as a current affirmative state.
  const own = status.ownership;
  const ownChip =
    own.state === "unknown" ? (
      <Chip tone="unknown" label={text.ownerUnknown} />
    ) : own.state !== "verified" ? (
      <Chip tone="warn" label={text.ownerUnverified} />
    ) : receiving ? (
      <Chip tone="fresh" label={text.ownerVerified} />
    ) : (
      <Chip tone="stale" label={text.ownerVerifiedOld} />
    );
  const ownDetail =
    own.state === "unknown"
      ? text.ownerUnknownDetail
      : own.state === "verified"
        ? text.ownerVerifiedDetail
        : reason(own.reason ?? "OWNER_UNVERIFIED", locale);

  const phase = status.session.phase;
  const matchChip =
    phase === "none" ? (
      <Chip tone="unknown" label={phases.none} />
    ) : phase === "unsupported" ? (
      <Chip tone="off" label={phases.unsupported} />
    ) : receiving ? (
      <Chip tone="fresh" label={phases[phase]} />
    ) : (
      <Chip tone="stale" label={`${phases[phase]} (${text.staleSuffix})`} />
    );
  const matchDetail =
    status.session.session_epoch !== null ? text.session(status.session.session_epoch) : text.noMatch;

  const mode = status.mode;
  const modeChip =
    mode.state === "identified" && mode.value ? (
      <Chip tone="fresh" label={mode.value} />
    ) : mode.state === "unsupported" ? (
      <Chip tone="off" label={phases.unsupported} />
    ) : (
      <Chip tone="stale" label={text.ownerUnverified} />
    );
  const modeDetail = mode.state === "unverified" ? text.modeUnknown : "";

  const media = status.media;
  const mediaChip =
    media.state === "ready" ? (
      <Chip tone="fresh" label={text.ready} />
    ) : media.state === "unavailable" ? (
      <Chip tone="off" label={text.intake.unavailable} />
    ) : (
      <Chip tone="warn" label={text.notReady} />
    );

  const capture = status.capture;
  const captureChip =
    capture.state === "available" ? (
      <Chip tone="fresh" label={text.ready} />
    ) : capture.state === "unsupported" ? (
      <Chip tone="off" label={phases.unsupported} />
    ) : (
      <Chip tone="off" label={text.intake.unavailable} />
    );

  const cpu = status.resource.cpu;
  const mem = status.resource.memory;
  const resourceChip =
    cpu.state === "unavailable" ? (
      <Chip tone="unknown" label={text.resourceUnavailable} />
    ) : mem.state === "over_budget" ? (
      <Chip tone="warn" label={text.ramOverBudget} />
    ) : cpu.throttle === "throttled" ? (
      <Chip tone="warn" label={text.throttled} />
    ) : (
      <Chip tone="fresh" label={text.normal} />
    );
  const resourceDetail = text.resourceDetail(cpu.last_sample_percent, megabytes(mem.working_set_bytes));

  const account = status.account.state;
  const accountChip =
    account === "signed_in" ? (
      <Chip tone="fresh" label={text.signedIn} />
    ) : account === "signed_out" ? (
      <Chip tone="unknown" label={text.signedOut} />
    ) : (
      <Chip tone="off" label={text.unused} />
    );

  return (
    <section className="card" aria-labelledby="ready-h">
      <div className="card-head">
        <h2 id="ready-h">{text.title}</h2>
        <span className="muted small">{text.note}</span>
      </div>
      <div role="table" aria-labelledby="ready-h">
        <Row name={text.intakeName} chip={intakeChip[i.state]} detail={intakeDetail} />
        <Row name={text.ownerName} chip={ownChip} detail={ownDetail} />
        <Row name={text.matchName} chip={matchChip} detail={matchDetail} />
        <Row name={text.modeName} chip={modeChip} detail={modeDetail} />
        <Row name={text.criticalMediaName} chip={mediaChip} detail={mediaReason(media.reason, locale)} />
        <Row name={text.mapVisibilityName} chip={captureChip} detail={reason(capture.reason, locale)} />
        <Row name={text.resourceName} chip={resourceChip} detail={resourceDetail} />
        <Row name={text.accountName} chip={accountChip} detail={text.accountDetail} />
      </div>
    </section>
  );
}
