import { HeroPortrait } from "../deck/HeroPortrait";
import { draftBoardCopy, formatWinRate, type DraftBoardRow, type DraftBoardSlot } from "./draftBoardModel";

function SlotView({ slot, locale }: { slot: DraftBoardSlot; locale: "th" | "en" }) {
  const text = draftBoardCopy(locale);
  const enemy = slot.ours === false;
  return (
    <li className={`sm-board-slot${slot.heroName ? "" : " is-unknown"}`} data-testid={`sm-board-${slot.slot}`}>
      <HeroPortrait heroId={slot.heroId} size="lg" alt={slot.heroName ?? "—"} />
      <b className="sm-board-hero" title={slot.heroName ?? undefined}>{slot.heroName ?? "—"}</b>
      <span className="sm-board-player" title={slot.playerName ?? undefined}>{slot.playerName ?? "—"}</span>
      <span className="sm-board-stat">
        <span className="sm-board-rate mono">{formatWinRate(slot.winRate)}</span>
        {slot.winRate !== null && <span className="sm-src">{text.estimate}</span>}
        {slot.lowData && <span className="sm-src is-pending">{text.lowData}</span>}
      </span>
      {slot.position !== null && (
        <span className="sm-board-pos">{enemy ? text.lane(slot.position) : text.position(slot.position)}
          <span className={`sm-src${enemy ? " is-pending" : ""}`}>{enemy ? text.guess : text.estimate}</span></span>
      )}
    </li>
  );
}

export function DraftBoard({ rows, locale, namesAvailable, statsAvailable }: {
  rows: DraftBoardRow[]; locale: "th" | "en"; namesAvailable: boolean; statsAvailable: boolean;
}) {
  const text = draftBoardCopy(locale);
  return (
    <section className="sm-board" aria-label={text.title} data-testid="sm-draft-board">
      {rows.map(row => (
        <div key={row.team} className={`sm-board-row sm-board-${row.team}${row.ours === true ? " is-ours" : row.ours === false ? " is-theirs" : ""}`}>
          <h2>
            {row.team === "radiant" ? text.radiant : text.dire}
            {row.ours !== null && <span className={row.ours ? "sm-board-ours" : "sm-enemy-mark"}>{row.ours ? text.ours : text.theirs}</span>}
          </h2>
          <ol>{row.slots.map(slot => <SlotView key={slot.slot} slot={slot} locale={locale} />)}</ol>
        </div>
      ))}
      <p className="sm-board-sources">
        <span className="sm-src">{text.source.draft}</span>
        <span className={`sm-src${namesAvailable ? "" : " is-pending"}`}>{namesAvailable ? text.source.names : text.source.namesOff}</span>
        <span className={`sm-src${statsAvailable ? "" : " is-pending"}`}>{statsAvailable ? text.source.stats : text.source.statsOff}</span>
      </p>
    </section>
  );
}
