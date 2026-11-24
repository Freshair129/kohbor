import type { PregameDashboardView } from "./pregameDashboardModel";
import type { MinimapState } from "../steam/model";

function stateLine(state: MinimapState, locale: "th" | "en"): string {
  const labels = locale === "th" ? {
    not_running: "Dota 2 ยังไม่เปิด — กดเริ่ม Dota 2",
    launching: "กำลังเปิด Dota 2",
    hero_selection: "รอเกมเริ่ม",
    loading: "กำลังโหลดเกม",
    first_frame: "กำลังเปิดภาพแผนที่",
    live: "รอข้อมูลเกม",
    occluded: "แผนที่ถูกบัง",
    unsupported: "รอข้อมูลเกม",
  } : {
    not_running: "Dota 2 is closed — press Start Dota 2",
    launching: "Launching Dota 2",
    hero_selection: "Waiting for the match",
    loading: "Loading the game",
    first_frame: "Opening the minimap",
    live: "Waiting for game data",
    occluded: "Minimap is covered",
    unsupported: "Waiting for game data",
  };
  return labels[state];
}

function dayValue(day: PregameDashboardView["today"], locale: "th" | "en"): string {
  const rate = day.winRate === null ? "—" : `${day.winRate}%`;
  return `${day.matches} · ${day.wins}/${day.losses} · ${rate}`;
}

export function PregameDashboard({ model, locale, mapState }: {
  model: PregameDashboardView;
  locale: "th" | "en";
  mapState: MinimapState;
}) {
  const th = locale === "th";
  return (
    <section className="live-centre-dashboard" aria-label={th ? "สรุปการเล่นก่อนเริ่มเกม" : "Pre-game play dashboard"} data-testid="live-pregame-dashboard">
      <header>
        <h2>{th ? "ก่อนเริ่มเกม" : "Before the match"}</h2>
        <p role="status">{stateLine(mapState, locale)}</p>
      </header>
      {model.state === "empty" ? <p className="live-dashboard-empty" role="status">{model.emptyMessage}</p>
        : model.state === "unavailable" ? <p className="live-dashboard-empty" role="status">{th ? "อ่านประวัติแมตช์ในเครื่องไม่ได้" : "Local match history is unavailable"}</p>
          : <>
            <div className="live-dashboard-day-grid">
              <article><h3>{th ? "วันนี้" : "Today"}</h3><p>{dayValue(model.today, locale)}</p></article>
              <article><h3>{th ? "เมื่อวาน" : "Yesterday"}</h3><p>{dayValue(model.yesterday, locale)}</p></article>
            </div>
            {model.scopeLabel && <p className="live-dashboard-scope">{model.scopeLabel}</p>}
            <article className="live-dashboard-last">
              <h3>{th ? "แมตช์ล่าสุด" : "Last match"}</h3>
              <div className="live-dashboard-last-row">
                <strong>{model.lastMatch?.heroName ?? "—"}</strong>
                <span>{model.lastMatch?.result === "win" ? (th ? "ชนะ" : "Win")
                  : model.lastMatch?.result === "loss" ? (th ? "แพ้" : "Loss") : "—"}</span>
                <span>K/D/A {model.lastMatch?.kda ?? "—"}</span>
                <span>{model.lastMatch?.duration ?? "—"}</span>
              </div>
            </article>
            {model.bestHeroThisWeek && <article className="live-dashboard-best">
              <h3>{th ? "ฮีโร่ผลงานดีที่สุดสัปดาห์นี้" : "Best hero this week"}</h3>
              <p><strong>{model.bestHeroThisWeek.heroName}</strong> · {model.bestHeroThisWeek.wins}/{model.bestHeroThisWeek.losses} · {model.bestHeroThisWeek.winRate}%</p>
              <small>{model.bestHeroThisWeek.games} {th ? "แมตช์" : "matches"}</small>
            </article>}
          </>}
    </section>
  );
}
