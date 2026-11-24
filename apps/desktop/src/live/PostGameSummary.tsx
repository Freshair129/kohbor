import type { PostGameSummaryView } from "./postGameSummaryModel";

export function PostGameSummary({ summary, locale, onClose, onOpenReplay }: {
  summary: PostGameSummaryView;
  locale: "th" | "en";
  onClose: () => void;
  onOpenReplay: (matchKey?: string) => void;
}) {
  const th = locale === "th";
  const recap = summary.recapEvents.slice(-4).reverse();
  return (
    <section className="live-postgame-summary" aria-label={th ? "สรุปหลังเกม" : "Post-game summary"} data-testid="live-postgame-summary">
      <header className="live-postgame-heading">
        <div><small>{th ? "สรุปแมตช์" : "MATCH SUMMARY"}</small><h2>{th ? "จบเกมแล้ว" : "Match complete"}</h2></div>
        <button type="button" onClick={onClose} aria-label={th ? "ปิดสรุปแมตช์" : "Close match summary"}>{th ? "ปิด" : "Close"}</button>
      </header>
      <div className="live-postgame-result-row">
        <strong>{summary.result === "win" ? (th ? "ชนะ" : "Win") : summary.result === "loss" ? (th ? "แพ้" : "Loss") : "—"}</strong>
        <span>{summary.heroName}</span><span>{th ? "เวลา" : "Duration"} {summary.duration}</span>
      </div>
      <dl className="live-postgame-stats">
        <div><dt>K / D / A</dt><dd>{summary.kda}</dd></div>
        <div><dt>LH / DN</dt><dd>{summary.lastHitsDenies}</dd></div>
        <div><dt>GPM / XPM</dt><dd>{summary.gpmXpm}</dd></div>
        <div><dt>{th ? "คะแนนสุดท้าย" : "Final score"}</dt><dd>{summary.finalScore}</dd></div>
        {summary.derivedStats.map(stat => <div key={stat.label}><dt>{stat.label}</dt><dd>{stat.value}</dd></div>)}
      </dl>
      <section className="live-postgame-events" aria-label={th ? "เหตุการณ์และเสียงประกาศในแมตช์" : "Match alerts and announcer events"}>
        <h3>{th ? "เหตุการณ์และเสียงประกาศ" : "Alerts and announcer events"}</h3>
        {recap.length ? <ul>{recap.map((event, index) => <li key={`${event.time}-${event.event}-${index}`}>
          <span>{event.time}</span><strong>{event.event}</strong><small>{event.outcome}</small>
        </li>)}</ul> : <p>{summary.recapAvailable
          ? (th ? "ยังไม่มีเหตุการณ์ที่บันทึกสำหรับแมตช์นี้" : "No match events are recorded yet")
          : (th ? "ยังไม่มีสรุปเหตุการณ์ของแมตช์นี้" : "Match recap is unavailable")}</p>}
        {summary.recapDroppedCount > 0 && <small>{th ? `ไม่นำเหตุการณ์ ${summary.recapDroppedCount} รายการมาแสดง` : `${summary.recapDroppedCount} events were not played`}</small>}
      </section>
      <section className="live-postgame-replay" aria-label={th ? "วิเคราะห์จากรีเพลย์" : "Replay analysis"}>
        {summary.replay ? <>
          <h3>{th ? "วิเคราะห์เต็มจากรีเพลย์" : "Full replay analysis"}</h3>
          <dl>{summary.replay.headline.map(item => <div key={item.label}><dt>{item.label}</dt><dd>{item.value}</dd></div>)}</dl>
        </> : <>
          <h3>{th ? "วิเคราะห์เต็มจากรีเพลย์" : "Full replay analysis"}</h3>
          <p>{th ? "เมื่อคลังรีเพลย์จับคู่แมตช์นี้ได้ ให้วางไฟล์ .dem ในโฟลเดอร์ที่เฝ้าดู หรือเพิ่มไฟล์ .dem" : "When the replay library can match this match, place a .dem file in the watched folder or add a .dem file."}</p>
        </>}
        {summary.replay && <button type="button" onClick={() => onOpenReplay(summary.replay?.openMatchKey)}>{th ? "เปิดวิเคราะห์แมตช์นี้" : "Open this match's analysis"}</button>}
      </section>
    </section>
  );
}
