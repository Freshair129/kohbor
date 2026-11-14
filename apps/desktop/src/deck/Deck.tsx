import { copy, seconds, type Locale } from "../copy";
import { Chip } from "../Readiness";
import { MinimapPanel } from "../live/MinimapPanel";
import { mediaTone } from "../mediaStatus";
import { useMediaListing } from "../useMediaListing";
import type { AudioMuteState } from "../AudioPanel";
import type { OwnSnapshot, RuntimeStatus } from "../types";
import { deckCopy, ALLY_SLOT_IDS, ENEMY_SLOT_IDS } from "./copy";
import type { DeckStateView } from "./deckState";
import { deriveOwnHeroViewModel, ownHeroLifeLabel } from "./ownHeroModel";
import "./deck.css";

function readableNumber(value: number | null): string {
  return value === null ? "—" : String(value);
}

export function Deck({
  locale,
  status,
  snapshot,
  state,
  mute,
}: {
  locale: Locale;
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  state: DeckStateView;
  mute: AudioMuteState;
}) {
  const text = deckCopy(locale);
  const own = deriveOwnHeroViewModel(snapshot);
  const stale = state.state === "stale" || own.snapshotAvailability === "stale";
  const hasLatestResult = state.state === "debrief" && own.hasOwnSnapshot;
  const hpWidth = own.hpPercent === null ? undefined : `${Math.max(0, Math.min(100, own.hpPercent))}%`;
  const manaWidth = own.manaPercent === null ? undefined : `${Math.max(0, Math.min(100, own.manaPercent))}%`;

  return (
    <main className="deck-root" lang={locale} data-state={state.state}>
      <section className="deck-column deck-own-column" aria-label={text.ownHero}>
        <section className={`deck-own-card${stale ? " is-stale" : ""}`} aria-labelledby="deck-own-heading">
          <header className="deck-card-heading">
            <h1 id="deck-own-heading">{text.ownHero}</h1>
            {hasLatestResult && <span className="deck-result-label">{text.latestResult}</span>}
          </header>

          <div className="deck-hero-identity">
            <span className="deck-hero-id" title={own.heroId ?? undefined}>{own.heroId ?? "—"}</span>
            <span className="deck-hero-level"><span>{text.level}</span> <b>{readableNumber(own.level)}</b></span>
            <span className={`deck-age-chip${stale ? " is-stale" : ""}`} aria-label={`${stale ? text.oldData : text.age}: ${seconds(own.ageMs, locale)}`}>
              {stale ? text.oldData : text.age} · {seconds(own.ageMs, locale)}
            </span>
          </div>

          <div className="deck-vitals">
            <div className="deck-vital-row">
              <span className="deck-vital-label">{text.hp}</span>
              <span className="deck-vital-value mono">{readableNumber(own.hp)} / {readableNumber(own.maxHp)}</span>
              <span className="deck-progress" role="img" aria-label={`${text.hp}: ${readableNumber(own.hp)} / ${readableNumber(own.maxHp)}`}>
                {hpWidth !== undefined && <span style={{ width: hpWidth }} />}
              </span>
            </div>
            <div className="deck-vital-row">
              <span className="deck-vital-label">{text.mana}</span>
              <span className="deck-vital-value mono">{readableNumber(own.manaPercent)}{own.manaPercent === null ? "" : "%"}</span>
              <span className="deck-progress deck-progress-mana" role="img" aria-label={`${text.mana}: ${readableNumber(own.manaPercent)}${own.manaPercent === null ? "" : "%"}`}>
                {manaWidth !== undefined && <span style={{ width: manaWidth }} />}
              </span>
            </div>
            <div className="deck-life-row">
              <span className="deck-vital-label">{text.alive}</span>
              <strong>{ownHeroLifeLabel(own, locale)}</strong>
            </div>
          </div>

          <section className="deck-stats" aria-label={text.statsHeading} aria-describedby="deck-stats-caption">
            {text.statLabels.map((label) => (
              <div className="deck-stat-tile" key={label}>
                <span>{label}</span>
                <b>—</b>
              </div>
            ))}
          </section>
          <small className="deck-stats-caption" id="deck-stats-caption">{text.statCaption}</small>
        </section>

        <section className="deck-ally-panel" aria-labelledby="deck-allies-heading">
          <header className="deck-card-heading">
            <h2 id="deck-allies-heading">{text.allies}</h2>
          </header>
          <ul className="deck-ally-list">
            {ALLY_SLOT_IDS.map((slot) => (
              <li className="deck-ally-slot" key={slot}>
                <span className="deck-slot-token">{slot}</span>
                <span className="deck-placeholder-copy">
                  <b>{text.placeholder.allies[locale === "en" ? "en" : "th"]}</b>
                  <small>{text.placeholder.allySource[locale === "en" ? "en" : "th"]}</small>
                </span>
              </li>
            ))}
          </ul>
        </section>
      </section>

      <section className="deck-column deck-map-column" aria-label={copy(locale).shell.tabs.live}>
        {state.state === "standby" && (
          <p className="deck-standby-note" role="status">{locale === "th" ? state.reason.th : state.reason.en}</p>
        )}
        <MinimapPanel locale={locale} layout="deck" />
      </section>

      <section className="deck-column deck-right-column" aria-label={text.enemies}>
        <section className="deck-enemy-panel" aria-labelledby="deck-enemies-heading">
          <header className="deck-card-heading">
            <h2 id="deck-enemies-heading">{text.enemies}</h2>
          </header>
          <ul className="deck-enemy-list">
            {ENEMY_SLOT_IDS.map((slot) => (
              <li className="deck-enemy-slot" key={slot}>
                <span className="deck-enemy-token" aria-hidden="true">{slot}</span>
                <span>{text.placeholder.enemies[locale === "en" ? "en" : "th"]}</span>
              </li>
            ))}
          </ul>
        </section>

        <EventFeed locale={locale} />
        <SoundPackStatus locale={locale} status={status} mute={mute} />
      </section>
    </main>
  );
}

function EventFeed({ locale }: { locale: Locale }) {
  const text = deckCopy(locale);
  return (
    <section className="deck-event-panel" aria-labelledby="deck-events-heading">
      <header className="deck-card-heading">
        <h2 id="deck-events-heading">{text.eventFeed}</h2>
      </header>
      <ul className="deck-event-list" aria-label={text.eventFeed}>
        <li>{text.placeholder.emptyEvents[locale === "en" ? "en" : "th"]}</li>
      </ul>
    </section>
  );
}

function SoundPackStatus({
  locale,
  status,
  mute,
}: {
  locale: Locale;
  status: RuntimeStatus | null;
  mute: AudioMuteState;
}) {
  const text = deckCopy(locale);
  const audioText = copy(locale).audio;
  const { listing, state } = useMediaListing();
  const media = status?.media ?? null;
  const mediaLabel = media === null ? audioText.mediaState.unknown : audioText.mediaState[media.state];
  const packName = state === "ok" && listing
    ? listing.active?.package_id ?? text.placeholder.noPack[locale === "en" ? "en" : "th"]
    : "—";
  const muteLabel = mute.muted === null ? "—" : mute.muted ? text.muted : text.unmuted;

  return (
    <section className="deck-sound-panel" aria-labelledby="deck-sound-heading">
      <header className="deck-card-heading">
        <h2 id="deck-sound-heading">{text.soundPack}</h2>
      </header>
      <div className="deck-sound-row">
        <span>{text.audioReadiness}</span>
        <Chip tone={mediaTone(media)} label={mediaLabel} />
      </div>
      <div className="deck-sound-row">
        <span>{text.activePack}</span>
        <strong title={state === "failed" ? "media_list unavailable" : undefined}>
          {packName}
        </strong>
      </div>
      <div className="deck-sound-row">
        <span>{text.mute}</span>
        <strong>{muteLabel}</strong>
      </div>
    </section>
  );
}
