// Overview page, variant D (owner decision 2026-11-23): own stats in the main column, Kohbor
// news in the side column. No readiness or GSI content here (OVERVIEW-001).
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import localHeroStats from "../assets/dota/6989/stats.json";
import { dotaIcon } from "../live/dotaIconSet";
import { overviewCopy, type OverviewLocale } from "./overviewCopy";
import { overviewClient, type OverviewClient } from "./overviewClient";
import {
  aggregateNemesis, bestHeroPanel, buildLocalHeroStyleTags, countingNotes, EMPTY_HISTORY, formatDate, formatDateTime,
  heroName as displayHeroName, localMonthStart, matchRows, NEWS_FILTERS, newsColumn, openDotaStatusText, parseMatchHistory, parseNemesisReply,
  parseNewsView, parseOpenDotaView, pick, placeholderRowCount, statTiles, todayBestHero,
  type BestHero, type MatchHistory, type NemesisMatch, type NemesisSummary, type NewsFilter, type NewsPost, type NewsView, type OpenDotaView, type TileView,
} from "./overviewModel";
import "./overview.css";

const RECENT_ROWS = 8;
const HISTORY_REFRESH_MS = 30_000;
const NEWS_LOADING_POLL_MS = 1_500;
const OPENDOTA_LOADING_POLL_MS = 1_000;
const OPENDOTA_ACCOUNT_POLL_MS = 5_000;

type Loaded<T> = { status: "loading" } | { status: "unavailable" } | { status: "ready"; value: T };

function HeroIcon({ hero, className }: { hero: string | null; className: string }) {
  const url = dotaIcon("hero", hero);
  return url ? <img className={className} src={url} alt="" /> : <span className={`${className} ov-hero-ph`} aria-hidden="true" />;
}

function Donut({ donut, label, centerLabel, locale }: { donut: TileView["donut"]; label: string; centerLabel: string; locale: OverviewLocale }) {
  const t = overviewCopy(locale).stats;
  const legend = [
    { kind: "win", text: t.win, value: donut.wins },
    { kind: "loss", text: t.loss, value: donut.losses },
    { kind: "none", text: t.donutNone, value: donut.noResult },
  ];
  return (
    <div className="ov-donut-wrap">
      <svg className="ov-donut" viewBox="0 0 42 42" role="img" aria-label={`${label}: ${legend.map((l) => `${l.text} ${l.value}`).join(", ")}`}>
        <circle className="ov-donut-track" cx="21" cy="21" r="15.915" pathLength={100} />
        {donut.segments.map((s) => (
          <circle key={s.kind} className={`ov-donut-seg ${s.kind}`} cx="21" cy="21" r="15.915" pathLength={100}
            strokeDasharray={`${s.length} ${100 - s.length}`} strokeDashoffset={25 - s.offset} />
        ))}
        <text x="21" y="21" className="ov-donut-num">{donut.total}</text>
        <text x="21" y="27" className="ov-donut-cap">{centerLabel}</text>
      </svg>
      <ul className="ov-legend">
        {legend.map((l) => <li key={l.kind}><i className={l.kind} />{l.text}<span>{l.value}</span></li>)}
      </ul>
    </div>
  );
}

function TrendFrame({ tiles, label }: { tiles: TileView; label: string }) {
  // Always the frame, axes and 0/50/100 gridlines; the line only from 10 counted matches.
  const y = (v: number) => 64 - 4 - (v / 100) * 56;
  return (
    <div className="ov-trend">
      <svg className="ov-spark" viewBox="0 0 300 64" preserveAspectRatio="none" role="img" aria-label={tiles.trend.label ?? tiles.trend.need ?? label}>
        <rect x="0" y="0" width="300" height="64" className="ov-spark-frame" vectorEffect="non-scaling-stroke" />
        {[0, 50, 100].map((v) => (
          <line key={v} x1="0" x2="300" y1={y(v)} y2={y(v)} className={v === 50 ? "ov-spark-mid" : "ov-spark-grid"} vectorEffect="non-scaling-stroke" />
        ))}
        {tiles.trend.points && <polyline points={tiles.trend.points} className="ov-spark-line" vectorEffect="non-scaling-stroke" />}
      </svg>
      <div className="ov-spark-axis" aria-hidden="true"><span>100%</span><span>50%</span><span>0%</span></div>
      {tiles.trend.need && <div className="ov-spark-need">{tiles.trend.need}</div>}
    </div>
  );
}

function BestHeroes({ title, heroes, minGames, locale }: { title: string; heroes: readonly BestHero[]; minGames: number; locale: OverviewLocale }) {
  const t = overviewCopy(locale).stats;
  const cards = bestHeroPanel(heroes, minGames, locale);
  return (
    <section className="card ov-best" aria-label={title}>
      <div className="ov-sec-title"><h3>{title}</h3></div>
      <div className="ov-best-grid">
        {cards.map((card) => (
          <div className={`ov-hcard${card.placeholder ? " placeholder" : ""}`} key={card.hero} title={card.title ?? undefined}>
            <HeroIcon hero={card.hero} className="ov-hcard-img" />
            <div className="ov-hcard-b">
              <div className="ov-hcard-n">
                <span title={card.name}>{card.name}</span>
                {card.placeholder && <span className="ov-ph-chip" title={card.title ?? undefined}>{t.placeholderChip}</span>}
              </div>
              <dl className="ov-hcard-stats">
                <div><dt>{t.winRateShort}</dt><dd>{card.rate}</dd></div>
                <div><dt>{t.columns.result}</dt><dd>{card.record}</dd></div>
                <div><dt>{t.avgKda}</dt><dd>{card.kda}</dd></div>
              </dl>
              <div className="ov-hcard-m"><span>{card.games}</span>{card.basis && <span className="ov-basis" title={card.basisTitle ?? undefined}>{card.basis}</span>}</div>
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

function BestHeroToday({ history, locale }: { history: MatchHistory; locale: OverviewLocale }) {
  const t = overviewCopy(locale).stats;
  const today = todayBestHero(history.entries, new Date(), history.best.minGames);
  const card = today.hero ? bestHeroPanel([today.hero], history.best.minGames, locale)[0] : null;
  const reason = today.reason === "no_matches_today" ? t.noMatchesToday
    : today.reason === "no_hero_today" ? t.noHeroToday : null;
  return (
    <section className="card ov-best ov-best-today" aria-label={t.bestToday}>
      <div className="ov-sec-title"><h3>{t.bestToday}</h3></div>
      <div className="ov-basis ov-today-rule">{t.bestTodayRule(history.best.minGames)}</div>
      {card ? (
        <div className="ov-today-card">
          <HeroIcon hero={card.hero} className="ov-today-img" />
          <div className="ov-today-body">
            <strong title={card.name}>{card.name}</strong>
            <span>{card.games} · {t.winRateShort}: {card.rate}</span>
            <span>{t.columns.result}: {card.record}</span>
            <span>{card.basisTitle}</span>
          </div>
        </div>
      ) : <div className="ov-today-empty"><strong>—</strong><span>{reason}</span></div>}
    </section>
  );
}

const localHeroStyleTags = buildLocalHeroStyleTags((localHeroStats as { heroes: unknown }).heroes);

function NemesisPanel({ state, locale }: { state: Loaded<{ matches: NemesisMatch[] }>; locale: OverviewLocale }) {
  const t = overviewCopy(locale).stats;
  const summary: NemesisSummary | null = state.status === "ready"
    ? aggregateNemesis(state.value, localHeroStyleTags) : null;
  return (
    <section className="card ov-nemesis" aria-label={t.nemesisTitle}>
      <div className="ov-sec-title"><h3>{t.nemesisTitle}</h3></div>
      {state.status === "loading" && <p className="ov-note" role="status">{t.nemesisLoading}</p>}
      {state.status === "unavailable" && <p className="ov-note" role="status">{t.nemesisUnavailable}</p>}
      {summary && summary.totalSamples === 0 && <p className="ov-note">{t.nemesisNoSamples}</p>}
      {summary && summary.totalSamples > 0 && (
        <>
          {summary.rows.length > 0 ? <ul className="ov-nemesis-list">
            {summary.rows.map((row) => (
              <li key={row.hero}>
                <HeroIcon hero={row.hero} className="ov-nemesis-img" />
                <span className="ov-nemesis-name">{displayHeroName(row.hero, locale)}</span>
                <span className="ov-nemesis-count">{t.nemesisCounts(row.lost, row.faced, row.sampleSize)}</span>
              </li>
            ))}
          </ul> : <p className="ov-note">{t.nemesisNoLosses}</p>}
          {summary.styles.length > 0 && <div className="ov-nemesis-styles">
            {summary.styles.map((style) => <p key={style.tag}>
              <span>{t.nemesisStylesLabel(style.samples)}</span>
              <strong>{t.nemesisStyleName(style.tag)} · {style.losses}/{style.samples}</strong>
            </p>)}
          </div>}
          {summary.lowData && <p className="ov-note ov-nemesis-low">{t.nemesisLowData(summary.minLanes)}</p>}
        </>
      )}
    </section>
  );
}

function HistoryTable({ rows, fillers, locale }: { rows: ReturnType<typeof matchRows>; fillers: number; locale: OverviewLocale }) {
  const t = overviewCopy(locale).stats;
  return (
    <table className="ov-mt">
      <thead><tr>
        <th scope="col">{t.columns.hero}</th><th scope="col">{t.columns.result}</th><th scope="col">{t.columns.kda}</th>
        <th scope="col">{t.columns.duration}</th><th scope="col" className="opt">{t.columns.date}</th><th scope="col">{t.columns.source}</th>
      </tr></thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.key} title={row.typeTitle ?? undefined}>
            <td className="h"><HeroIcon hero={row.hero} className="ov-mt-img" /><span>{row.heroName}</span></td>
            <td><span className={`ov-res ${row.resultTone}`} title={row.resultTitle}>{row.result}</span></td>
            <td className="num">{row.kda}</td>
            <td className="num">{row.duration}</td>
            <td className="opt muted">{row.date}</td>
            <td><span className="ov-src" title={t.sourceTitle}>{row.source}</span></td>
          </tr>
        ))}
        {Array.from({ length: fillers }, (_, i) => (
          <tr key={`empty-${i}`} className="ov-row-empty" aria-hidden="true">
            <td className="h"><span className="ov-mt-img ov-hero-ph" /><span>—</span></td>
            <td><span className="ov-res unknown">—</span></td>
            <td className="num">— / — / —</td><td className="num">—</td><td className="opt muted">—</td><td>—</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function OpenDotaCard({ locale, client, historyChanged, cacheRevision }: {
  locale: OverviewLocale; client: OverviewClient; historyChanged: () => void; cacheRevision: number;
}) {
  const t = overviewCopy(locale).stats;
  const [view, setView] = useState<OpenDotaView | null>(null);
  const [failed, setFailed] = useState(false);
  const [saving, setSaving] = useState(false);

  const read = useCallback(async () => {
    const raw = await client.openDotaView();
    const parsed = parseOpenDotaView(raw);
    if (parsed) setView(parsed);
    return parsed;
  }, [client]);

  const refresh = useCallback(async (retry: boolean) => {
    setFailed(false);
    setView((current) => current ? { ...current, state: "loading" } : {
      state: "loading", enabled: true, cachedEntries: 0, fetchedAt: null, lastAttemptAt: null, retryAvailable: false,
    });
    try {
      const parsed = parseOpenDotaView(await client.refreshOpenDota(retry));
      if (parsed) setView(parsed);
      historyChanged();
    } catch {
      setFailed(true);
      setView((current) => current ? { ...current, state: "offline" } : {
        state: "offline", enabled: true, cachedEntries: 0, fetchedAt: null, lastAttemptAt: null, retryAvailable: true,
      });
    }
  }, [client, historyChanged]);

  const pollForAccount = useCallback(async () => {
    try {
      const parsed = parseOpenDotaView(await client.refreshOpenDota(false));
      if (!parsed) {
        setFailed(true);
        return;
      }
      setFailed(false);
      setView(parsed);
      if (parsed.state !== "waiting_for_account") historyChanged();
    } catch {
      setFailed(true);
    }
  }, [client, historyChanged]);

  useEffect(() => {
    let alive = true;
    client.openDotaPrefs().then(async (prefs) => {
      if (!alive) return;
      if (!prefs.enabled) {
        setView({ state: "disabled", enabled: false, cachedEntries: 0, fetchedAt: null, lastAttemptAt: null, retryAvailable: false });
        return;
      }
      const cached = await read();
      if (!alive) return;
      if (!cached || cached.state !== "loading") await refresh(false);
    }).catch(() => {
      if (alive) setView({ state: "unavailable", enabled: false, cachedEntries: 0, fetchedAt: null, lastAttemptAt: null, retryAvailable: false });
    });
    return () => { alive = false; };
  }, [client, read, refresh, cacheRevision]);

  useEffect(() => {
    if (view?.state === "waiting_for_account") {
      const timer = window.setInterval(() => { void pollForAccount(); }, OPENDOTA_ACCOUNT_POLL_MS);
      return () => window.clearInterval(timer);
    }
    if (view?.state !== "loading") return;
    const timer = window.setInterval(() => { void read().catch(() => setFailed(true)); }, OPENDOTA_LOADING_POLL_MS);
    return () => window.clearInterval(timer);
  }, [pollForAccount, read, view?.state]);

  const change = async (next: boolean) => {
    setSaving(true);
    setFailed(false);
    try {
      const prefs = await client.setOpenDotaEnabled(next);
      if (!prefs.enabled) {
        setView({ state: "disabled", enabled: false, cachedEntries: 0, fetchedAt: null, lastAttemptAt: null, retryAvailable: false });
        historyChanged();
      } else {
        await refresh(false);
      }
    } catch {
      setFailed(true);
    } finally {
      setSaving(false);
    }
  };

  const state = view?.state ?? "idle";
  const status = openDotaStatusText(state, locale);
  const showsCacheTime = view !== null && view.cachedEntries > 0 && view.fetchedAt !== null
    && ["loading", "offline", "rate_limited", "budget_exhausted", "private_profile", "not_found", "merged"].includes(state);

  return (
    <div className="ov-notice" role="note">
      <div>
        <strong>{t.openDotaTitle}</strong>
        <span>{t.openDotaDisclosure}</span>
        <span>{t.openDotaBody}</span>
        <span role="status">{failed ? t.openDotaUnavailable : status}</span>
        {state === "private_profile" && <span><strong>{t.openDotaPrivateHint}</strong></span>}
        {showsCacheTime && <span>{t.openDotaCachedAt(formatDateTime(view.fetchedAt!, locale))}</span>}
      </div>
      <div className="ov-row-actions">
        <label className="ov-optin">
          <input type="checkbox" checked={view?.enabled === true} disabled={view === null || saving}
            onChange={(event) => void change(event.currentTarget.checked)} />
          <span>{view?.enabled ? t.openDotaDisable : t.openDotaEnable}</span>
        </label>
        {view?.enabled && ["private_profile", "not_found", "rate_limited", "budget_exhausted", "offline", "unavailable"].includes(state)
          && view.retryAvailable && <button type="button" className="quiet" disabled={saving} onClick={() => void refresh(true)}>{t.openDotaRetry}</button>}
      </div>
    </div>
  );
}

function StatsColumn({ locale, client }: { locale: OverviewLocale; client: OverviewClient }) {
  const t = overviewCopy(locale).stats;
  const [history, setHistory] = useState<Loaded<MatchHistory>>({ status: "loading" });
  const [showAll, setShowAll] = useState(false);
  const historyDialog = useRef<HTMLDialogElement>(null);
  const [nemesis, setNemesis] = useState<Loaded<{ matches: NemesisMatch[] }>>({ status: "loading" });
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearFailed, setClearFailed] = useState(false);
  const [cacheRevision, setCacheRevision] = useState(0);

  const load = useCallback(() => {
    client.matchHistory(localMonthStart(new Date()))
      .then((raw) => {
        const parsed = parseMatchHistory(raw);
        setHistory(parsed ? { status: "ready", value: parsed } : { status: "unavailable" });
      })
      .catch(() => setHistory({ status: "unavailable" }));
  }, [client]);

  const loadNemesis = useCallback(() => {
    client.nemesisData()
      .then((raw) => {
        const parsed = parseNemesisReply(raw);
        setNemesis(parsed ? { status: "ready", value: parsed } : { status: "unavailable" });
      })
      .catch(() => setNemesis({ status: "unavailable" }));
  }, [client]);

  useEffect(() => {
    load();
    const timer = window.setInterval(load, HISTORY_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  useEffect(() => {
    loadNemesis();
    const timer = window.setInterval(loadNemesis, HISTORY_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [loadNemesis]);

  useEffect(() => {
    const dialog = historyDialog.current;
    if (!dialog) return;
    if (showAll && !dialog.open) dialog.showModal();
    else if (!showAll && dialog.open) dialog.close();
  }, [showAll]);

  const historyChanged = useCallback(() => load(), [load]);

  const clear = () => {
    setClearFailed(false);
    client.clearMatchHistory()
      .then(() => { setConfirmClear(false); setCacheRevision((revision) => revision + 1); load(); })
      .catch(() => setClearFailed(true));
  };

  // Owner 2026-11-23 (DEC-OVERVIEW rev 3): the full layout always renders; loading, unavailable
  // and empty histories show zeros plus a one-line note instead of collapsing sections.
  const h = history.status === "ready" ? history.value : EMPTY_HISTORY;
  const note = history.status === "loading" ? t.loading : history.status === "unavailable" ? t.unavailable
    : h.entries.length === 0 ? t.noMatches : null;
  const tiles = statTiles(h, locale);
  const rows = matchRows(h, locale, RECENT_ROWS);
  const allRows = matchRows(h, locale, null);
  const fillers = placeholderRowCount(rows.length);

  return (
    <div className="ov-main">
      <h2 className="ov-col-title">{t.title}</h2>
      <OpenDotaCard locale={locale} client={client} historyChanged={historyChanged} cacheRevision={cacheRevision} />
      {note && <p className="ov-note ov-state-note" role="status">{note}</p>}
      <div className="ov-tiles">
        <section className="card ov-tile" aria-label={t.overall}>
          <div className="ov-lbl">{t.overall}</div>
          {tiles.overall.scope && <div className="ov-scope">{tiles.overall.scope}</div>}
          <div className={`ov-val${tiles.overall.known ? "" : " na"}`}>{tiles.overall.value}</div>
          <div className="ov-sub">{tiles.overall.sub}</div>
        </section>
        <section className="card ov-tile" aria-label={t.donut}>
          <div className="ov-lbl">{t.donut}</div>
          <Donut donut={tiles.donut} label={t.donut} centerLabel={t.donutCenter} locale={locale} />
        </section>
        <section className="card ov-tile" aria-label={tiles.last.title}>
          <div className="ov-lbl">{tiles.last.title}</div>
          <div className={`ov-val${h.last.winRate === null ? " na" : ""}`}>{tiles.last.value}</div>
          <div className="ov-sub">{tiles.last.sub}</div>
          <div className="ov-wl" role="img" aria-label={tiles.last.sub}>
            {tiles.last.slots.map((r, i) => <i key={i} className={r === "win" ? "w" : r === "loss" ? "l" : "empty"} title={r === "win" ? t.win : r === "loss" ? t.loss : undefined} />)}
          </div>
        </section>
        <section className="card ov-tile" aria-label={t.trend}>
          <div className="ov-lbl">{t.trend}</div>
          <TrendFrame tiles={tiles} label={t.trend} />
        </section>
      </div>
      <ul className="ov-count-notes ov-note">
        {countingNotes(h, locale).map((line) => <li key={line}>{line}</li>)}
      </ul>

      <div className="ov-best-row">
        <BestHeroes title={t.bestAllTime} heroes={h.best.allTime} minGames={h.best.minGames} locale={locale} />
        <BestHeroes title={t.bestMonth} heroes={h.best.thisMonth} minGames={h.best.minGames} locale={locale} />
        <BestHeroToday history={h} locale={locale} />
      </div>

      <NemesisPanel state={nemesis} locale={locale} />

      <section className="card">
        <div className="ov-sec-title">
          <h3>{t.recent}</h3>
          {h.entries.length > RECENT_ROWS && (
            <button type="button" className="quiet" onClick={() => setShowAll(true)} aria-expanded={showAll} aria-haspopup="dialog">
              {t.showAll(h.entries.length)}
            </button>
          )}
        </div>
        <div className="ov-table-wrap" role="region" aria-label={t.recent} tabIndex={0}>
          <HistoryTable rows={rows} fillers={fillers} locale={locale} />
        </div>
        {h.entries.length > 0 && (
          <div className="ov-clear">
            {confirmClear ? (
              <>
                <span className="ov-note">{t.clearConfirm}</span>
                <button type="button" className="danger" onClick={clear}>{t.clearYes}</button>
                <button type="button" className="quiet" onClick={() => setConfirmClear(false)}>{t.clearNo}</button>
              </>
            ) : <button type="button" className="quiet" onClick={() => setConfirmClear(true)}>{t.clear}</button>}
            {clearFailed && <span className="ov-note" role="alert">{t.clearFailed}</span>}
          </div>
        )}
      </section>

      <dialog ref={historyDialog} className="ov-history-dialog" aria-label={t.fullHistory} onClose={() => setShowAll(false)}>
        <div className="ov-dialog-head">
          <h3>{t.fullHistory}</h3>
          <button type="button" className="quiet" onClick={() => setShowAll(false)}>{t.closeHistory}</button>
        </div>
        <div className="ov-table-wrap ov-history-dialog-table" role="region" aria-label={t.fullHistory} tabIndex={0}>
          <HistoryTable rows={allRows} fillers={0} locale={locale} />
        </div>
      </dialog>
    </div>
  );
}

function Post({ post, locale, big, client }: { post: NewsPost; locale: OverviewLocale; big: boolean; client: OverviewClient }) {
  const t = overviewCopy(locale).news;
  const [open, setOpen] = useState(false);
  const [linkNote, setLinkNote] = useState<string | null>(null);
  const openLink = (index: number) => {
    setLinkNote(null);
    client.openNewsLink(post.id, index)
      .then((reply) => {
        if (!reply.opened) setLinkNote(reply.reason === "MATCH_ACTIVE" ? t.linkBlocked : t.linkFailed);
      })
      .catch(() => setLinkNote(t.linkFailed));
  };
  return (
    <article className={`card ov-post${post.pinned ? " pinned" : ""}`}>
      {big && post.image?.dataUrl && (
        <img className="ov-post-img" src={post.image.dataUrl} alt={pick(post.image.alt, locale)}
          width={post.image.width || undefined} height={post.image.height || undefined} />
      )}
      <div className="ov-post-top">
        <span className={`ov-tchip t-${post.type}`}>{t.types[post.type]}</span>
        {post.pinned && <span>{t.pinned}</span>}
        <span>{formatDate(post.publishedAt, locale)}</span>
        {post.appVersion && <span className="mono">{post.appVersion}</span>}
        {post.gamePatch && <span className="mono">{post.gamePatch}</span>}
      </div>
      <h3>{pick(post.title, locale)}</h3>
      <p>{pick(post.summary, locale)}</p>
      {open && post.body && <p className="ov-post-body">{pick(post.body, locale)}</p>}
      {(post.body || post.links.length > 0) && (
        <div className="ov-post-more">
          {post.body && <button type="button" className="link" onClick={() => setOpen((v) => !v)} aria-expanded={open}>{open ? t.readLess : t.readMore}</button>}
          {post.links.map((link) => (
            <button type="button" className="link" key={link.index} onClick={() => openLink(link.index)}>{pick(link.label, locale)} ↗</button>
          ))}
        </div>
      )}
      {linkNote && <p className="ov-note" role="status">{linkNote}</p>}
    </article>
  );
}

function NewsColumnView({ locale, client }: { locale: OverviewLocale; client: OverviewClient }) {
  const t = overviewCopy(locale).news;
  const [view, setView] = useState<Loaded<NewsView>>({ status: "loading" });
  const [filter, setFilter] = useState<NewsFilter>("all");
  const [expanded, setExpanded] = useState(false);

  const load = useCallback(() => {
    client.news()
      .then((raw) => {
        const parsed = parseNewsView(raw);
        setView(parsed ? { status: "ready", value: parsed } : { status: "unavailable" });
      })
      .catch(() => setView({ status: "unavailable" }));
  }, [client]);

  useEffect(() => { load(); }, [load]);
  const loading = view.status === "ready" && view.value.state === "loading";
  useEffect(() => {
    if (!loading) return;
    const timer = window.setInterval(load, NEWS_LOADING_POLL_MS);
    return () => window.clearInterval(timer);
  }, [loading, load]);

  const setPrefs = (patch: { enabled?: boolean; disclosure_seen?: boolean }) => {
    client.setNewsPrefs(patch).then(load, load);
  };
  const retry = () => { client.refreshNews().then(load, load); };

  const skeleton = (
    <div aria-busy="true" aria-label={t.loading} role="status">
      {[0, 1, 2].map((i) => <article key={i} className="card ov-post"><div className="ov-skel w30" /><div className="ov-skel w85 tall" /><div className="ov-skel" /><div className="ov-skel w70" /></article>)}
    </div>
  );

  let body: ReactNode;
  if (view.status === "loading") body = skeleton;
  else if (view.status === "unavailable") body = <div className="ov-empty"><strong>{t.unavailableTitle}</strong></div>;
  else {
    const v = view.value;
    if (v.state === "disabled") {
      body = (
        <div className="ov-empty">
          <strong>{t.disabledTitle}</strong>
          <p className="ov-note">{t.disabledBody}</p>
          <button type="button" className="quiet" onClick={() => setPrefs({ enabled: true, disclosure_seen: true })}>{t.enable}</button>
        </div>
      );
    } else if (v.state === "loading" && v.posts.length === 0) body = skeleton;
    else if (v.state === "unavailable") {
      body = (
        <div className="ov-notice warn">
          <div><strong>{t.unavailableTitle}</strong><span>{t.unavailableBody}</span></div>
          <button type="button" className="quiet" onClick={retry}>{t.retry}</button>
        </div>
      );
    } else {
      const column = newsColumn(v.posts, filter, expanded);
      const shown = column.pinned ? [column.pinned, ...column.posts] : column.posts;
      body = (
        <>
          {v.state === "offline" && (
            <div className="ov-notice warn">
              <div><strong>{t.offlineTitle}</strong><span>{t.offlineBody(v.fetchedAt === null ? "—" : formatDateTime(v.fetchedAt, locale))}</span></div>
              <button type="button" className="quiet" onClick={retry}>{t.retry}</button>
            </div>
          )}
          {v.posts.length > 0 && (
            <div className="ov-filters" role="radiogroup" aria-label={t.filterLabel}>
              {NEWS_FILTERS.map((f) => (
                <button key={f} type="button" role="radio" aria-checked={filter === f} className={filter === f ? "on" : ""} onClick={() => setFilter(f)}>
                  {t.types[f]}<span>{column.counts[f]}</span>
                </button>
              ))}
            </div>
          )}
          {v.posts.length === 0 ? (
            <div className="ov-empty"><strong>{t.emptyTitle}</strong><p className="ov-note">{t.emptyBody}</p></div>
          ) : shown.length === 0 ? (
            <div className="ov-empty"><p className="ov-note">{t.emptyFiltered}</p></div>
          ) : shown.map((post, i) => <Post key={post.id} post={post} locale={locale} big={i === 0 || expanded} client={client} />)}
          {(column.hidden > 0 || expanded) && (
            <button type="button" className="quiet ov-all-news" onClick={() => setExpanded((e) => !e)} aria-expanded={expanded}>
              {expanded ? t.fewerNews : `${t.allNews} (${column.hidden}) ›`}
            </button>
          )}
          {v.fetchedAt !== null && v.state === "ok" && <p className="ov-note">{t.updated(formatDateTime(v.fetchedAt, locale))}</p>}
        </>
      );
    }
  }

  const disclosure = view.status === "ready" && view.value.enabled && !view.value.disclosureSeen ? (
    <div className="ov-notice" role="note">
      <div><strong>{t.disclosureTitle}</strong><span>{t.disclosureBody}</span></div>
      <div className="ov-row-actions">
        <button type="button" className="quiet" onClick={() => setPrefs({ disclosure_seen: true })}>{t.disclosureOk}</button>
        <button type="button" className="quiet" onClick={() => setPrefs({ enabled: false, disclosure_seen: true })}>{t.turnOff}</button>
      </div>
    </div>
  ) : null;

  return (
    <aside className="ov-side" aria-label={t.title}>
      <h2 className="ov-col-title">{t.title}</h2>
      {disclosure}
      {body}
    </aside>
  );
}

export function OverviewPage({ locale, client = overviewClient }: { locale: OverviewLocale; client?: OverviewClient }) {
  return (
    <div className="shell-screen ov-page">
      <StatsColumn locale={locale} client={client} />
      <NewsColumnView locale={locale} client={client} />
    </div>
  );
}
