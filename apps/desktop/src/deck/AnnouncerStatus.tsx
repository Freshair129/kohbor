import { copy, type Locale } from "../copy";
import { Chip } from "../Readiness";
import { mediaTone } from "../mediaStatus";
import { useMediaListing } from "../useMediaListing";
import type { AudioMuteState } from "../AudioPanel";
import type { AudioHistory, HistoryLoadState } from "../history";
import type { PlaybackState } from "../playback";
import type { RuntimeStatus } from "../types";
import { deckCopy } from "./copy";
import { deriveLastClip, deriveOnAir, shortPlaybackId } from "./eventFeedModel";

export function AnnouncerStatus({ locale, status, mute, history, historyState, hostNowMs, playback, hostLost }: {
  locale: Locale; status: RuntimeStatus | null; mute: AudioMuteState; history: AudioHistory | null;
  historyState: HistoryLoadState; hostNowMs: number | null; playback: PlaybackState; hostLost: boolean;
}) {
  const text = deckCopy(locale);
  const { listing, state } = useMediaListing();
  const media = hostLost ? null : status?.media ?? null;
  const mediaLabel = media === null ? copy(locale).audio.mediaState.unknown : copy(locale).audio.mediaState[media.state];
  const packName = state === "ok" && listing && !hostLost
    ? listing.active ? `${text.packActive} · ${listing.active.package_id}` : text.placeholder.noPack[locale] : "—";
  const last = deriveLastClip(history, historyState, hostNowMs, locale);
  const onAir = hostLost ? null : deriveOnAir(playback, locale);
  return (
    <section className="deck-sound-panel" aria-labelledby="deck-sound-heading">
      <header className="deck-card-heading"><h2 id="deck-sound-heading">{text.soundPack}</h2></header>
      <div className="deck-sound-row"><span>{text.audioReadiness}</span><Chip tone={mediaTone(media)} label={mediaLabel} /></div>
      <div className="deck-sound-row"><span>{text.activePack}</span><strong title={listing?.active?.package_id}>{packName}</strong></div>
      <div className="deck-sound-row"><span>{text.mute}</span><strong>{mute.muted === null || hostLost ? "—" : mute.muted ? text.muted : text.unmuted}</strong></div>
      <div className="deck-sound-row deck-last-clip"><span>{text.lastClip}</span>
        <strong title={last?.record.playback_id ? `${last.eventLabel} · ${last.record.playback_id} · ${last.age}` : undefined}>
          {last?.record.playback_id ? <><span className="deck-clip-name">{last.demo ? "DEMO · " : ""}{last.eventLabel} · {shortPlaybackId(last.record.playback_id)}</span><span className="deck-clip-age">{last.age}</span></> : "—"}
        </strong>
      </div>
      {onAir && <div className="deck-sound-row deck-on-air"><span>ON AIR</span><strong title={onAir.playbackId}>{onAir.eventLabel} · {shortPlaybackId(onAir.playbackId)}</strong></div>}
    </section>
  );
}
