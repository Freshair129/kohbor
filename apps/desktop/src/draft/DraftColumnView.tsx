import {
  bracketLabel,
  dataStateDetailText,
  dataStateText,
  draftHiddenReasonText,
  failureText,
  formatDraftTime,
  formatFormTenths,
  formatGames,
  formatWinRateTenths,
  overlaySuggestionRows,
  pickCardStateText,
  positionLabel,
  staleTimesText,
  suggestionRowView,
  unsetContextLabel,
  type DraftColumnRow,
  type DraftColumnView as DraftColumnViewModel,
  type DraftDurationCell,
  type PickCardState,
} from "./draftColumnModel";
import "./draft-column.css";

type Surface = "overlay" | "deck";

function publicValuesAvailable(view: DraftColumnViewModel): boolean {
  return (view.data_state === "live" || view.data_state === "stale") && view.source?.fetched_at_ms !== null;
}

function suggestionNotes(view: DraftColumnViewModel) {
  const draftBar = view.suggestions?.draft_bar;
  if (!draftBar) return "สถานะ Draft Bar ยังไม่ทราบ";
  if (!draftBar.available) return "Kohbor ยังไม่เห็นว่าตัวไหนถูกแบนหรือถูกเลือกแล้ว";
  const identified = draftBar.identified === null ? "—" : String(draftBar.identified);
  const slots = draftBar.slots === null ? "—" : String(draftBar.slots);
  return `เห็นฮีโร่ที่ระบุได้ ${identified}/${slots} · ยังไม่เห็นตัวที่ถูกแบน`;
}

function SuggestionRow({ row, stale }: { row: DraftColumnRow; stale: boolean }) {
  const item = suggestionRowView(row);
  const showRank = item.rank !== null;
  const showRate = item.lowDataLabel === null && item.unknownGamesLabel === null;
  return (
    <li className={`draft-row${item.taken ? " is-taken" : ""}${item.lowDataLabel ? " is-low-data" : ""}${showRank ? " has-rank" : ""}${showRate ? " has-rate" : ""}`}>
      {showRank && <span className="draft-row-rank" aria-hidden="true">{item.rank}</span>}
      <span className="draft-row-name">
        <strong>{row.name}</strong>
        <small className="draft-row-games">
          {item.unknownGamesLabel ?? `${item.lowDataLabel ? `${item.lowDataLabel} · ` : ""}${item.gamesText} เกม`}
        </small>
      </span>
      {showRate && <span className={`draft-row-rate draft-rate-${item.band}${stale ? " is-stale-value" : ""}`}>{item.winRateText}</span>}
    </li>
  );
}

function SourceFooter({ view }: { view: DraftColumnViewModel }) {
  const source = view.source;
  return (
    <footer className="draft-column-footer">
      <span>ที่มา: {source?.provider ?? "—"} · {source?.window ?? "ไม่ระบุช่วง"}</span>
      <span>ดึงเมื่อ {formatDraftTime(source?.fetched_at_ms ?? null)} · กลุ่มตำแหน่งเป็นค่าประมาณ</span>
    </footer>
  );
}

function PickSourceFooter({ view, games }: { view: DraftColumnViewModel; games: number | null }) {
  const source = view.source;
  return (
    <footer className="draft-column-footer">
      <span>ที่มา: {source?.provider ?? "—"} · {source?.window ?? "ไม่ระบุช่วง"} · {bracketLabel(view.context.bracket)}</span>
      <span>ดึงเมื่อ {formatDraftTime(source?.fetched_at_ms ?? null)} · {formatGames(games)} เกม</span>
    </footer>
  );
}

function DraftContext({ view, surface }: { view: DraftColumnViewModel; surface: Surface }) {
  return (
    <div className="draft-context-block">
      <div className="draft-context">
        <strong>{positionLabel(view.context.position)}</strong>
        <span>{bracketLabel(view.context.bracket)}</span>
      </div>
      <p className="draft-context-hint">{surface === "overlay" ? "ตั้งเอง · เปลี่ยนใน Kohbor" : "คุณตั้งไว้เอง · เปลี่ยนที่หน้าต่าง Kohbor"}</p>
      {unsetContextLabel(view.context) && <p className="draft-context-unset">{unsetContextLabel(view.context)}</p>}
    </div>
  );
}

function SuggestionsCard({ view, synthetic, surface }: { view: DraftColumnViewModel; synthetic: boolean; surface: Surface }) {
  const state = view.data_state;
  const stale = state === "stale";
  const showRows = publicValuesAvailable(view) && state !== "bracket_empty";
  const hostRows = showRows ? view.suggestions?.rows ?? [] : [];
  const rows = surface === "overlay" ? overlaySuggestionRows(hostRows) : hostRows;
  const statusText = dataStateText(view);
  const staleText = surface === "overlay"
    ? `ข้อมูล ${formatDraftTime(view.source?.fetched_at_ms ?? null)} · ล้มเหลว ${formatDraftTime(view.source?.failed_at_ms ?? null)}`
    : dataStateDetailText(view);
  return (
    <section className={`draft-column-card draft-suggestions-card${stale ? " is-stale" : ""}`} aria-label="สถิติฮีโร่">
      {synthetic && <div className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</div>}
      <header className="draft-column-header">
        <div className="draft-column-mark" aria-hidden="true">K</div>
        <strong>ฮีโร่ชนะบ่อย</strong>
        {surface === "deck" && statusText && <span className={`draft-state-flag draft-state-${state ?? "unknown"}`}>{statusText}</span>}
      </header>
      {surface === "overlay" && statusText && (
        <div className="draft-overlay-state"><span className={`draft-state-flag draft-state-${state ?? "unknown"}`}>{statusText}</span></div>
      )}
      <p className="draft-column-subtitle">ช่วงนี้ · สถิติย้อนหลัง ไม่ใช่คำทำนาย</p>
      <DraftContext view={view} surface={surface} />
      <div className="draft-row-heading"><span>ฮีโร่ · จำนวนเกม</span><span>อัตราชนะ</span></div>
      {state === "loading" && <div className="draft-loading-copy">กำลังขอสถิติจาก OpenDota…</div>}
      {state !== "loading" && !showRows && (
        <div className="draft-state-message">
          <strong>{dataStateText(view) ?? "ไม่มีสถิติที่แสดงได้"}</strong>
          {dataStateDetailText(view) && <small>{dataStateDetailText(view)}</small>}
        </div>
      )}
      {showRows && rows.length > 0 && (
        <ul className="draft-row-list">
          {rows.map((row) => <SuggestionRow key={row.hero_id} row={row} stale={stale} />)}
        </ul>
      )}
      {showRows && rows.length === 0 && <p className="draft-empty-rows">ไม่มีรายการที่แสดงได้</p>}
      {stale && <p className="draft-stale-times">{staleText}</p>}
      <div className="draft-column-notes">
        <span>{surface === "overlay" ? "อัตราชนะรวม · ในแรงก์นี้ทุกตำแหน่ง" : "อัตราชนะรวมของฮีโร่ในแรงก์นี้ ทุกตำแหน่ง"}</span>
        <span>{suggestionNotes(view)}</span>
      </div>
      <SourceFooter view={view} />
    </section>
  );
}

function durationLabel(cell: DraftDurationCell, state: PickCardState, fetchedAt: number | null) {
  const labels = { short: "ก่อน 30 นาที", mid: "30–45 นาที", long: "หลัง 45 นาที" } as const;
  const enough = cell.games !== null && cell.games >= 300;
  const available = (state === "live" || state === "stale") && fetchedAt !== null && enough;
  return {
    label: labels[cell.bin],
    rate: available ? formatWinRateTenths(cell.win_rate_tenths) : "—",
    games: formatGames(cell.games),
  };
}

function pickFailureDetail(state: PickCardState, view: DraftColumnViewModel, surface: Surface): string | null {
  const source = view.source;
  if (state === "stale") return surface === "overlay"
    ? `ข้อมูล ${formatDraftTime(source?.fetched_at_ms ?? null)} · ล้มเหลว ${formatDraftTime(source?.failed_at_ms ?? null)}`
    : staleTimesText(view);
  if (state === "fetch_failed") {
    return `${failureText(source?.failure ?? null, source?.http_status ?? null) ?? "—"} · ${formatDraftTime(source?.failed_at_ms ?? null)} · ไม่มีสถิติที่เก็บไว้ในเครื่อง`;
  }
  if (state === "rate_limited") return dataStateDetailText({ ...view, data_state: "rate_limited" });
  if (state === "provider_error") return `HTTP ${source?.http_status ?? "—"} · ${formatDraftTime(source?.failed_at_ms ?? null)}`;
  return null;
}

function PickCard({ view, synthetic, surface }: { view: DraftColumnViewModel; synthetic: boolean; surface: Surface }) {
  const card = view.pick_card;
  if (!card) return <div className="draft-state-message">ข้อมูลการ์ดไม่ครบ</div>;
  const stale = card.state === "stale";
  const canShowRate = (card.state === "live" || stale) && card.games !== null && card.games >= 1000 &&
    view.source?.fetched_at_ms !== null;
  const stateText = card.games !== null && card.games < 1000 && (card.state === "live" || stale)
    ? "ข้อมูลน้อย · ต่ำกว่า 1,000 เกม"
    : pickCardStateText(card.state);
  const durationsStale = card.durations.state === "stale";
  const durationCells = card.durations.cells.map((cell) => durationLabel(cell, card.durations.state, card.durations.fetched_at_ms));
  const showDurationRates = (card.durations.state === "live" || card.durations.state === "stale") && card.durations.fetched_at_ms !== null;
  const durationFailure = failureText(view.source?.failure ?? null, view.source?.http_status ?? null);
  const durationStaleText = surface === "overlay"
    ? `ข้อมูล ${formatDraftTime(card.durations.fetched_at_ms)} · ล้มเหลว ${formatDraftTime(view.source?.failed_at_ms ?? null)}`
    : `ข้อมูลของ ${formatDraftTime(card.durations.fetched_at_ms)} · ดึงใหม่ไม่ได้เมื่อ ${formatDraftTime(view.source?.failed_at_ms ?? null)}${durationFailure ? ` · ${durationFailure}` : ""}`;
  return (
    <section className={`draft-column-card draft-pick-card${stale ? " is-stale" : ""}`} aria-label="สถิติฮีโร่ที่เลือก">
      {synthetic && <div className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</div>}
      <header className="draft-column-header">
        <div className="draft-column-mark" aria-hidden="true">K</div>
        <strong>ตัวเลือกของคุณ</strong>
        <span className={`draft-state-flag draft-state-${card.state}`}>{stateText}</span>
      </header>
      <DraftContext view={view} surface={surface} />
      <div className="draft-pick-hero">
        <strong>{card.name ?? "—"}</strong>
        <span className={`draft-pick-rate${stale ? " is-stale-value" : ""}`}>{canShowRate ? formatWinRateTenths(card.win_rate_tenths) : "—"}</span>
      </div>
      <div className="draft-pick-metric-caption">อัตราชนะ · win rate · {formatGames(card.games)} เกม</div>
      <div className="draft-pick-form">
        <span>ฟอร์มย้อนหลัง</span>
        <strong>{canShowRate ? formatFormTenths(card.form_tenths) : "—"}</strong>
      </div>
      {card.state === "not_enough_matches" && <p className="draft-state-detail">{stateText} · {formatGames(card.games)} เกม</p>}
      {pickFailureDetail(card.state, view, surface) && <p className="draft-state-detail">{pickFailureDetail(card.state, view, surface)}</p>}
      <div className="draft-duration-heading">
        <span>อัตราชนะตามความยาวเกม</span>
        {durationsStale && <span className="draft-state-flag draft-state-stale">ข้อมูลเก่า</span>}
      </div>
      {(card.durations.state === "live" || durationsStale) && (
        <p className="draft-duration-meta">ดึงสถิติความยาวเกมเมื่อ {formatDraftTime(card.durations.fetched_at_ms)}</p>
      )}
      {durationsStale && (
        <p className="draft-stale-times">{durationStaleText}</p>
      )}
      <div className="draft-duration-grid">
        {durationCells.map((cell) => (
          <div className={`draft-duration-cell${durationsStale ? " is-stale" : ""}`} key={cell.label}>
            <strong className={stale || durationsStale ? "is-stale-value" : ""}>{showDurationRates ? cell.rate : "—"}</strong>
            <span>{cell.label}</span>
            <small>{cell.games} เกม</small>
          </div>
        ))}
      </div>
      {card.durations.state !== "live" && card.durations.state !== "stale" && (
        <p className="draft-state-detail">สถิติความยาวเกม: {pickCardStateText(card.durations.state)}</p>
      )}
      {stale && <p className="draft-stale-times">{pickFailureDetail(card.state, view, surface)}</p>}
      <PickSourceFooter view={view} games={card.games} />
    </section>
  );
}

export function DraftColumnView({ view, surface, synthetic = false }: {
  view: DraftColumnViewModel;
  surface: Surface;
  synthetic?: boolean;
}) {
  if (surface === "overlay" && (!view.valid || !view.overlay.visible || view.mode === "hidden")) return null;
  if (!view.valid) {
    if (surface === "overlay") return null;
    return (
      <section className="draft-column-deck-hidden" aria-label="สถิติฮีโร่สาธารณะ">
        <strong>ข้อมูลจากโฮสต์ไม่ถูกต้อง</strong>
      </section>
    );
  }
  if (view.mode === "hidden") {
    if (surface === "overlay") return null;
    const reason = draftHiddenReasonText(view.overlay.hidden_reason);
    return (
      <section className="draft-column-deck-hidden" aria-label="สถิติฮีโร่สาธารณะ">
        {synthetic && <div className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</div>}
        <strong>{reason ?? "ยังไม่มีข้อมูลสถิติฮีโร่ในหน้าดราฟ"}</strong>
        {!view.placement_verified && <span className="draft-placement-warning">ตำแหน่งการ์ดช่วงวางแผนยังไม่ได้วัด</span>}
      </section>
    );
  }
  if (view.mode === "consent_chip") {
    const reason = surface === "deck" ? draftHiddenReasonText(view.overlay.hidden_reason) : null;
    const placementWarning = surface === "deck" && !view.placement_verified
      ? "ตำแหน่งการ์ดช่วงวางแผนยังไม่ได้วัด"
      : null;
    return (
      <div className={`draft-column-surface draft-column-surface-${surface}`}>
        {reason && <p className="draft-hidden-reason">{reason}</p>}
        {placementWarning && <p className="draft-placement-warning">{placementWarning}</p>}
        <div className={`draft-consent-chip${surface === "overlay" ? " is-overlay" : ""}`}>
          {synthetic && <span className="draft-synthetic-flag">ตัวอย่างสังเคราะห์</span>}
          <strong>คำแนะนำฮีโร่ปิดอยู่ · เปิดได้ที่หน้าต่าง Kohbor</strong>
        </div>
      </div>
    );
  }
  const card = view.mode === "pick_card"
    ? <PickCard view={view} synthetic={synthetic} surface={surface} />
    : <SuggestionsCard view={view} synthetic={synthetic} surface={surface} />;
  const placementWarning = surface === "deck" && !view.placement_verified
    ? <p className="draft-placement-warning">ตำแหน่งการ์ดช่วงวางแผนยังไม่ได้วัด</p>
    : null;
  const hiddenReason = surface === "deck" ? draftHiddenReasonText(view.overlay.hidden_reason) : null;
  return (
    <div className={`draft-column-surface draft-column-surface-${surface}`}>
      {hiddenReason && <p className="draft-hidden-reason">{hiddenReason}</p>}
      {placementWarning}
      {card}
    </div>
  );
}
