import { copy, seconds, type Locale } from "../copy";
import { MinimapPanel } from "../live/MinimapPanel";
import { useAudioHistory } from "../useAudioHistory";
import { hostNowFromAnchor } from "../hostClock";
import type { PlaybackView } from "../runtime";
import type { AudioMuteState } from "../AudioPanel";
import type { OwnSnapshot, RuntimeStatus } from "../types";
import { deckCopy, ALLY_SLOT_IDS, ENEMY_SLOT_IDS } from "./copy";
import type { DeckStateView } from "./deckState";
import { deriveOwnHeroViewModel, ownHeroLifeLabel } from "./ownHeroModel";
import { AlertStrip } from "./AlertStrip";
import { EventFeed } from "./EventFeed";
import { AnnouncerStatus } from "./AnnouncerStatus";
import { HeroPortrait } from "./HeroPortrait";
import type { SteamStatus } from "../steam/model";
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
  playback,
  hostLost,
  steamStatus,
}: {
  locale: Locale;
  status: RuntimeStatus | null;
  snapshot: OwnSnapshot | null;
  state: DeckStateView;
  mute: AudioMuteState;
  playback: PlaybackView;
  hostLost: boolean;
  steamStatus: SteamStatus | null;
}) {
  const text = deckCopy(locale);
  const own = deriveOwnHeroViewModel(snapshot);
  // A retained payload's age is not a current age once the host stops answering.
  const displayAgeMs = hostLost ? null : own.ageMs;
  const history = useAudioHistory(hostLost);
  const historyState = hostLost ? "failed" : history.state;
  const hostNowMs = hostLost ? null : hostNowFromAnchor(playback.clockAnchor, performance.now());
  const statValues = [readableNumber(own.stats.gold),
    [own.stats.kills, own.stats.deaths, own.stats.assists].map(readableNumber).join(" / "),
    [own.stats.last_hits, own.stats.denies].map(readableNumber).join(" / "),
    [own.stats.gpm, own.stats.xpm].map(readableNumber).join(" / "), "—", "—"];
  const stale = state.state === "stale" || own.snapshotAvailability === "stale";
  const hasLatestResult = state.state === "debrief" && own.hasOwnSnapshot;
  const hpWidth = own.hpPercent === null ? undefined : `${Math.max(0, Math.min(100, own.hpPercent))}%`;
  const manaWidth = own.manaPercent === null ? undefined : `${Math.max(0, Math.min(100, own.manaPercent))}%`;

  return (
    <main className="deck-root" lang={locale} data-state={state.state}>
      <section className="deck-column deck-own-column" aria-label={text.ownHero}>
        <section className={`deck-own-card${stale ? " is-stale" : ""}`} aria-labelledby="deck-own-heading">
          <div className="deck-own-portrait"><HeroPortrait heroId={own.heroId} size="lg" alt={own.heroName?.name ?? text.ownHero} /></div>
          <header className="deck-card-heading">
            <h1 id="deck-own-heading">{text.ownHero}</h1>
            {hasLatestResult && <span className="deck-result-label">{text.latestResult}</span>}
          </header>

          <div className="deck-hero-identity">
            <div className="deck-hero-title">
              <strong className="deck-hero-name" title={own.heroName?.rawId}>{own.heroName?.name ?? "—"}</strong>
              {own.heroCaption && <small className="deck-hero-raw" title={own.heroCaption}>{own.heroCaption}</small>}
            </div>
            <span className="deck-hero-level"><span>{text.level}</span> <b>{readableNumber(own.level)}</b></span>
            <span className={`deck-age-chip${stale ? " is-stale" : ""}`} aria-label={`${stale ? text.oldData : text.age}: ${seconds(displayAgeMs, locale)}`}>
              {stale ? text.oldData : text.age} · {seconds(displayAgeMs, locale)}
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
            {text.statLabels.map((label, index) => (
              <div className="deck-stat-tile" key={label}>
                <span>{label}</span>
                <b>{statValues[index]}</b>
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
        <MinimapPanel locale={locale} layout="deck" steamStatus={steamStatus} renderAlerts={({ risk, captureLive }) => (
          <AlertStrip own={{ ...own, ageMs: displayAgeMs }} state={state.state} risk={risk} captureLive={captureLive} locale={locale} />
        )} />
      </section>

      <section className="deck-column deck-right-column" aria-label={text.enemies}>
        <section className="deck-enemy-panel" aria-labelledby="deck-enemies-heading">
          <header className="deck-card-heading">
            <h2 id="deck-enemies-heading">{text.enemies}</h2>
          </header>
          <p className="deck-enemy-caption" title={text.enemyCaption}>{text.enemyCaption}</p>
          <ul className="deck-enemy-list">
            {ENEMY_SLOT_IDS.map((slot) => (
              <li className="deck-enemy-slot" key={slot}>
                <span className="deck-enemy-token" aria-hidden="true">{slot}</span>
                <span>{text.placeholder.enemies[locale === "en" ? "en" : "th"]}</span>
              </li>
            ))}
          </ul>
        </section>

        <EventFeed locale={locale} history={history.history} state={historyState} hostNowMs={hostNowMs} />
        <AnnouncerStatus locale={locale} status={status} mute={mute} history={history.history}
          historyState={historyState} hostNowMs={hostNowMs} playback={playback} hostLost={hostLost} />
      </section>
    </main>
  );
}
