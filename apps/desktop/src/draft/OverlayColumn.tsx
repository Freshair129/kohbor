import {
  bracketLabel,
  counterHeading,
  dataStateDetailText,
  dataStateText,
  failureText,
  formatDraftTime,
  formatFormTenths,
  formatGames,
  formatWinRateTenths,
  overlayPill,
  overlaySections,
  overlayValuesAvailable,
  pickCardStateText,
  positionLabel,
  staleTimesText,
  suggestionRowView,
  type DraftColumnRow,
  type DraftColumnView as DraftColumnViewModel,
  type DraftCounterRow,
  type DraftLane,
} from "./draftColumnModel";
import { OverlayPortrait } from "./OverlayPortrait";
import type { ReactNode } from "react";
import "./draft-overlay.css";

/*
 * In-game column, Option A (owner choice 2026-11-22): sectioned cards, body text >= 12 px.
 * Every block has a fixed height (see OVERLAY_LAYOUT and hero_stats::layout); text is clamped
 * or ellipsized inside its block so the host can size the native window from the view alone.
 */

function Brand({ view, pill, synthetic }: { view: DraftColumnViewModel; pill: ReturnType<typeof overlayPill>; synthetic: boolean }) {
  const position = view.context.position === null ? "ทุกตำแหน่ง" : `ตำแหน่ง ${view.context.position}`;
  return (
    <div className="draft-ov-brand">
      <span className="draft-ov-mark" aria-hidden="true">K</span>
      <span className="draft-ov-context">
        {synthetic ? <span className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</span> : `${position} · ${bracketLabel(view.context.bracket)}`}
      </span>
      <span className={`draft-ov-pill is-${pill.tone}`}>{pill.text}</span>
    </div>
  );
}

function Head({ title, sub, tone = "own" }: { title: ReactNode; sub: ReactNode; tone?: "own" | "enemy" }) {
  return (
    <div className={`draft-ov-head is-${tone}`}>
      <h2>{title}</h2>
      <p>{sub}</p>
    </div>
  );
}

function Footer({ lines }: { lines: string[] }) {
  return (
    <footer className="draft-ov-card draft-ov-footer">
      {lines.map((line) => <span key={line}>{line}</span>)}
    </footer>
  );
}

/*
 * Card v3 (owner Variant B, 2026-11-23): a 70 px card with the hero's game portrait fading into a
 * solid card, a win-rate edge (Command Deck ally above 50 %, critical below, bone near 50 %), the
 * bracket win rate with its distance from 50 %, and the hero's numbers in the selected lane.
 */
type Tone = "up" | "flat" | "down";

function winTone(rateTenths: number | null): Tone {
  if (rateTenths === null) return "flat";
  if (rateTenths - 500 >= 10) return "up";
  if (rateTenths - 500 <= -10) return "down";
  return "flat";
}

function deltaText(rateTenths: number): string {
  const delta = rateTenths - 500;
  return `${delta >= 0 ? "+" : "−"}${(Math.abs(delta) / 10).toFixed(1)}`;
}

const LANE_LABEL = { safe: "Safe", mid: "Mid", off: "Off" } as const;

function CardShell({ heroKey, tone, className, children }: { heroKey: string | null; tone: Tone; className: string; children: ReactNode }) {
  return (
    <li className={`${className} draft-ov-hero-card`}>
      <OverlayPortrait heroKey={heroKey} />
      <span className="draft-ov-fade" aria-hidden="true" />
      <span className={`draft-ov-edge is-${tone}`} aria-hidden="true" />
      <span className="draft-ov-rowbody">{children}</span>
    </li>
  );
}

function RateLine({ rateTenths, stale }: { rateTenths: number; stale: boolean }) {
  const tone = winTone(rateTenths);
  return (
    <span className="draft-ov-line2">
      <span className={`draft-row-rate is-${tone}${stale ? " is-stale-value" : ""}`}>{formatWinRateTenths(rateTenths)}</span>
      <span className={`draft-ov-chip is-${tone}`}>{deltaText(rateTenths)}</span>
    </span>
  );
}

/** "Off 71% · ชนะ 51.9%": share of this hero's lane games and its win rate in that lane. */
function LaneLine({ lane }: { lane: DraftLane }) {
  const share = lane.share_tenths === null ? "—" : `${Math.round(lane.share_tenths / 10)}%`;
  return (
    <small className="draft-ov-pos">
      {`${LANE_LABEL[lane.lane]} ${share} · ชนะ `}
      <span className={`is-${winTone(lane.win_rate_tenths)}`}>{lane.win_rate_tenths === null ? "—" : formatWinRateTenths(lane.win_rate_tenths)}</span>
    </small>
  );
}

function MetaRow({ row, stale }: { row: DraftColumnRow; stale: boolean }) {
  const item = suggestionRowView(row);
  const showRate = item.lowDataLabel === null && item.unknownGamesLabel === null && row.win_rate_tenths !== null;
  const games = item.unknownGamesLabel ?? `${item.lowDataLabel ? "ข้อมูลน้อย · " : ""}${item.gamesText} เกม`;
  const tone = showRate ? winTone(row.win_rate_tenths) : "flat";
  return (
    <CardShell heroKey={row.hero_key ?? null} tone={tone}
      className={`draft-row${item.taken ? " is-taken" : ""}${item.lowDataLabel ? " is-low-data" : ""}`}>
      <span className="draft-row-name"><strong>{row.name}</strong></span>
      {showRate
        ? <RateLine rateTenths={row.win_rate_tenths!} stale={stale} />
        : <span className="draft-ov-line2"><small className="draft-row-games">{games}</small></span>}
      {row.lane
        ? <LaneLine lane={row.lane} />
        : <small className="draft-ov-pos">{showRate ? `${item.gamesText} เกม` : "ยังไม่ได้ตั้งตำแหน่ง"}</small>}
    </CardShell>
  );
}

function CounterCard({ rows, heading, stale }: { rows: DraftCounterRow[]; heading: string; stale: boolean }) {
  return (
    <section className="draft-ov-card is-enemy draft-counter-block" aria-label="แนะนำตัวแก้ทาง">
      <Head tone="enemy" title={heading} sub="ตาราง Kohbor · ไม่ใช่สถิติ" />
      <ul className="draft-ov-list">
        {rows.map((row) => {
          const rate = row.win_rate_tenths ?? null;
          return (
            <CardShell key={row.hero_id} heroKey={row.hero_key ?? null} tone={winTone(rate)} className="draft-counter-row">
              <span className="draft-row-name"><strong>{row.name}</strong></span>
              {rate !== null
                ? <RateLine rateTenths={rate} stale={stale} />
                : <span className="draft-ov-line2"><small className="draft-row-games">ไม่มีสถิติแรงก์นี้</small></span>}
              <small className="draft-ov-pos draft-ov-why">{row.against.length > 1 ? `แก้ ${row.against.join(" · ")} · ${row.reason}` : row.reason}</small>
            </CardShell>
          );
        })}
      </ul>
    </section>
  );
}

const POSITION_ROLE = ["Safe Lane", "Mid", "Offlane", "Soft Support", "Hard Support"] as const;
const POSITION_LANE: Record<number, { lane: string; shared: number | null }> = {
  1: { lane: "Safe", shared: 5 }, 2: { lane: "Mid", shared: null }, 3: { lane: "Off", shared: 4 },
  4: { lane: "Off", shared: 3 }, 5: { lane: "Safe", shared: 1 },
};

/** Footer caveat for the lane numbers: all brackets, and a lane two positions share. */
function laneCaveat(position: number | null): string {
  const entry = position === null ? undefined : POSITION_LANE[position];
  if (position === null || !entry) return "ยังไม่ได้ตั้งตำแหน่ง · ไม่มีเลข Pos";
  const positions = entry.shared === null ? `${position}` : `${Math.min(position, entry.shared)}+${Math.max(position, entry.shared)}`;
  return `Pos ${positions} = เลน ${entry.lane} ทุกแรงก์`;
}

function metaSub(view: DraftColumnViewModel): string {
  if (view.data_state === "stale") return `ข้อมูลเก่า · ล้มเหลว ${formatDraftTime(view.source?.failed_at_ms ?? null)}`;
  const position = view.context.position;
  const pos = position !== null && position >= 1 && position <= 5 ? `Pos ${position} · ${POSITION_ROLE[position - 1]}` : "ทุกตำแหน่ง";
  const bar = view.suggestions?.draft_bar;
  if (!bar || !bar.available) return `${pos} · ยังไม่เห็นดราฟต์`;
  return `${pos} · เห็น ${bar.identified ?? "—"}/${bar.slots ?? "—"}`;
}

function SuggestionsColumn({ view, synthetic }: { view: DraftColumnViewModel; synthetic: boolean }) {
  const { counters, rows } = overlaySections(view);
  const counterBlock = view.suggestions?.counters ?? null;
  const stale = view.data_state === "stale";
  const values = overlayValuesAvailable(view);
  const detail = view.data_state === "fetch_failed"
    ? `${failureText(view.source?.failure ?? null, view.source?.http_status ?? null) ?? "—"} · ${formatDraftTime(view.source?.failed_at_ms ?? null)} · ลองใหม่ใน Kohbor`
    : dataStateDetailText(view);
  return (
    <>
      <Brand view={view} pill={overlayPill(view.data_state)} synthetic={synthetic} />
      {counters.length > 0 && counterBlock && <CounterCard rows={counters} heading={counterHeading(counterBlock)} stale={stale} />}
      <section className={`draft-ov-card draft-suggestions-card${stale ? " is-stale" : ""}`} aria-label="ฮีโร่เมต้า">
        <Head title="ฮีโร่เมต้า" sub={metaSub(view)} />
        {rows.length > 0
          ? <ul className="draft-ov-list draft-row-list">{rows.map((row) => <MetaRow key={row.hero_id} row={row} stale={stale} />)}</ul>
          : (
            <div className="draft-ov-message">
              <strong className="draft-ov-state">{values ? "ไม่มีรายการที่แสดงได้" : dataStateText(view) ?? "ไม่มีสถิติที่แสดงได้"}</strong>
              {!values && detail && <small>{detail}</small>}
            </div>
          )}
      </section>
      <Footer lines={[
        `ที่มา: ${view.source?.provider ?? "—"} · ${view.source?.window ?? "ไม่ระบุช่วง"}`,
        `ดึงเมื่อ ${formatDraftTime(view.source?.fetched_at_ms ?? null)}`,
        "อัตราชนะรวม · ไม่ใช่คำทำนาย",
        "กลุ่มตำแหน่งเป็นค่าประมาณ",
        laneCaveat(view.context.position),
      ]} />
    </>
  );
}

function pickDetail(view: DraftColumnViewModel, lowData: boolean): string {
  const card = view.pick_card!;
  const source = view.source;
  if (lowData) return `ข้อมูลน้อย · ต่ำกว่า 1,000 เกม (${formatGames(card.games)} เกม)`;
  switch (card.state) {
    case "live": return `อัตราชนะ · win rate · ${positionLabel(view.context.position)}`;
    case "stale": return staleTimesText(view) ?? "ข้อมูลเก่า";
    case "fetch_failed":
      return `${pickCardStateText(card.state)} · ${failureText(source?.failure ?? null, source?.http_status ?? null) ?? "—"} · ${formatDraftTime(source?.failed_at_ms ?? null)}`;
    case "rate_limited": return dataStateDetailText({ ...view, data_state: "rate_limited" }) ?? pickCardStateText(card.state);
    case "provider_error": return `${pickCardStateText(card.state)} · HTTP ${source?.http_status ?? "—"}`;
    default: return pickCardStateText(card.state);
  }
}

function PickColumn({ view, synthetic }: { view: DraftColumnViewModel; synthetic: boolean }) {
  const card = view.pick_card;
  if (!card) return null;
  const stale = card.state === "stale";
  const valuesState = card.state === "live" || stale;
  const lowData = valuesState && card.games !== null && card.games < 1000;
  const canShowRate = valuesState && !lowData && card.games !== null && view.source?.fetched_at_ms !== null;
  const durationsStale = card.durations.state === "stale";
  const durationValues = (card.durations.state === "live" || durationsStale) && card.durations.fetched_at_ms !== null;
  const labels = { short: "ก่อน 30 นาที", mid: "30–45 นาที", long: "หลัง 45 นาที" } as const;
  const durationSub = durationsStale
    ? `ข้อมูลเก่า · ${formatDraftTime(card.durations.fetched_at_ms)}`
    : card.durations.state === "live"
      ? `ดึงเมื่อ ${formatDraftTime(card.durations.fetched_at_ms)}`
      : `สถิติความยาวเกม: ${pickCardStateText(card.durations.state)}`;
  return (
    <>
      <Brand view={view} pill={overlayPill(card.state, lowData)} synthetic={synthetic} />
      <section className={`draft-ov-card draft-pick-card${stale ? " is-stale" : ""}`} aria-label="การ์ดฮีโร่">
        <Head title="การ์ดฮีโร่" sub={<span className="draft-ov-state">{lowData ? "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม" : card.state === "live" ? `ฮีโร่ที่คุณเลือก · ${bracketLabel(view.context.bracket)}` : pickCardStateText(card.state)}</span>} />
        <div className="draft-ov-pick">
          <strong className="draft-ov-hero">{card.name ?? "—"}</strong>
          <div className="draft-ov-stat"><span>อัตราชนะ (ย้อนหลัง)</span><strong className={stale ? "is-stale-value" : ""}>{canShowRate ? formatWinRateTenths(card.win_rate_tenths) : "—"}</strong></div>
          <div className="draft-ov-stat"><span>จำนวนเกม</span><strong>{formatGames(card.games)}</strong></div>
          <div className="draft-ov-stat"><span>ฟอร์มย้อนหลัง</span><strong className={stale ? "is-stale-value" : ""}>{canShowRate ? formatFormTenths(card.form_tenths) : "—"}</strong></div>
          <small className="draft-ov-detail">{pickDetail(view, lowData)}</small>
        </div>
      </section>
      <section className="draft-ov-card" aria-label="ตามความยาวเกม">
        <Head title="ตามความยาวเกม" sub={durationSub} />
        <ul className="draft-ov-durations">
          {card.durations.cells.map((cell) => {
            const enough = cell.games !== null && cell.games >= 300;
            return (
              <li key={cell.bin} className={`draft-duration-cell${durationsStale ? " is-stale" : ""}`}>
                <span>{labels[cell.bin]}</span>
                <strong className={stale || durationsStale ? "is-stale-value" : ""}>{durationValues && enough ? formatWinRateTenths(cell.win_rate_tenths) : "—"}</strong>
                <small>{formatGames(cell.games)} เกม</small>
              </li>
            );
          })}
        </ul>
      </section>
      <Footer lines={[
        `ที่มา: ${view.source?.provider ?? "—"} · ${view.source?.window ?? "ไม่ระบุช่วง"}`,
        `ดึงเมื่อ ${formatDraftTime(view.source?.fetched_at_ms ?? null)}`,
        `${formatGames(card.games)} เกม · ${bracketLabel(view.context.bracket)}`,
        "สถิติย้อนหลัง · ไม่ใช่คำทำนาย",
        "ความยาวเกม: ทุกแรงก์",
      ]} />
    </>
  );
}

export function OverlayColumn({ view, synthetic = false }: { view: DraftColumnViewModel; synthetic?: boolean }) {
  if (!view.valid || !view.overlay.visible || view.mode === "hidden") return null;
  return (
    <div className="draft-column-surface draft-column-surface-overlay draft-ov">
      {view.mode === "consent_chip" && (
        <div className="draft-ov-card draft-consent-chip is-overlay">
          <span className="draft-ov-mark" aria-hidden="true">K</span>
          {synthetic && <span className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</span>}
          <strong>คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor</strong>
        </div>
      )}
      {view.mode === "suggestions" && <SuggestionsColumn view={view} synthetic={synthetic} />}
      {view.mode === "pick_card" && <PickColumn view={view} synthetic={synthetic} />}
    </div>
  );
}
