// Overview page, variant D (owner decision 2026-11-23): own stats in the main column, Kohbor
// news in the side column. No readiness or GSI content here (OVERVIEW-001).
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { dotaIcon } from "../live/dotaIconSet";
import { overviewCopy, type OverviewLocale } from "./overviewCopy";
import { overviewClient, type OverviewClient } from "./overviewClient";
import {
  countingNotes, formatDate, formatDateTime, heroCards, matchRows, NEWS_FILTERS, newsColumn, parseMatchHistory,
  parseNewsView, pick, statTiles, type MatchHistory, type NewsFilter, type NewsPost, type NewsView,
} from "./overviewModel";
import "./overview.css";

const RECENT_ROWS = 8;
const HISTORY_REFRESH_MS = 30_000;
const NEWS_LOADING_POLL_MS = 1_500;

type Loaded<T> = { status: "loading" } | { status: "unavailable" } | { status: "ready"; value: T };

function HeroIcon({ hero, className }: { hero: string | null; className: string }) {
  const url = dotaIcon("hero", hero);
  return url ? <img className={className} src={url} alt="" /> : <span className={`${className} ov-hero-ph`} aria-hidden="true" />;
}

function StatsColumn({ locale, client }: { locale: OverviewLocale; client: OverviewClient }) {
  const t = overviewCopy(locale).stats;
  const [history, setHistory] = useState<Loaded<MatchHistory>>({ status: "loading" });
  const [showAll, setShowAll] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearFailed, setClearFailed] = useState(false);

  const load = useCallback(() => {
    client.matchHistory()
      .then((raw) => {
        const parsed = parseMatchHistory(raw);
        setHistory(parsed ? { status: "ready", value: parsed } : { status: "unavailable" });
      })
      .catch(() => setHistory({ status: "unavailable" }));
  }, [client]);

  useEffect(() => {
    load();
    const timer = window.setInterval(load, HISTORY_REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [load]);

  const clear = () => {
    setClearFailed(false);
    client.clearMatchHistory()
      .then(() => { setConfirmClear(false); load(); })
      .catch(() => setClearFailed(true));
  };

  const openDota = (
    <div className="ov-notice" role="note">
      <div><strong>{t.openDotaTitle}</strong><span>{t.openDotaBody}</span></div>
      <button type="button" className="quiet" disabled aria-disabled="true">{t.openDotaButton}</button>
    </div>
  );

  if (history.status !== "ready") {
    return (
      <div className="ov-main">
        <h2 className="ov-col-title">{t.title}</h2>
        {openDota}
        <section className="card ov-empty" role="status"><p className="ov-note">{history.status === "loading" ? t.loading : t.unavailable}</p></section>
      </div>
    );
  }

  const h = history.value;
  const tiles = statTiles(h, locale);
  const heroes = heroCards(h, locale);
  const rows = matchRows(h, locale, showAll ? null : RECENT_ROWS);

  return (
    <div className="ov-main">
      <h2 className="ov-col-title">{t.title}</h2>
      {openDota}
      <div className="ov-tiles">
        <section className="card ov-tile" aria-label={t.overall}>
          <div className="ov-lbl">{t.overall}</div>
          <div className={`ov-val${tiles.overall.known ? "" : " na"}`}>{tiles.overall.value}</div>
          <div className="ov-sub">{tiles.overall.sub}</div>
        </section>
        <section className="card ov-tile" aria-label={tiles.last.title}>
          <div className="ov-lbl">{tiles.last.title}</div>
          <div className={`ov-val${h.last.winRate === null ? " na" : ""}`}>{tiles.last.value}</div>
          {tiles.last.sub && <div className="ov-sub">{tiles.last.sub}</div>}
          <div className="ov-wl" role="img" aria-label={tiles.last.sub ?? tiles.last.title}>
            {tiles.last.squares.length > 0
              ? tiles.last.squares.map((r, i) => <i key={i} className={r === "win" ? "w" : "l"} title={r === "win" ? t.win : t.loss} />)
              : Array.from({ length: 20 }, (_, i) => <i key={i} />)}
          </div>
        </section>
        <section className="card ov-tile" aria-label={t.trend}>
          <div className="ov-lbl">{t.trend}</div>
          {tiles.trend.points ? (
            <svg className="ov-spark" viewBox="0 0 300 64" preserveAspectRatio="none" role="img" aria-label={tiles.trend.label ?? t.trend}>
              <line x1="0" x2="300" y1="32" y2="32" className="ov-spark-mid" vectorEffect="non-scaling-stroke" />
              <polyline points={tiles.trend.points} className="ov-spark-line" vectorEffect="non-scaling-stroke" />
            </svg>
          ) : <div className="ov-spark-need ov-note">{tiles.trend.need}</div>}
        </section>
      </div>
      <ul className="ov-count-notes ov-note">
        {countingNotes(h, locale).map((note) => <li key={note}>{note}</li>)}
      </ul>

      {h.entries.length === 0 ? (
        <section className="card ov-empty">
          <strong>{t.emptyTitle}</strong>
          <p className="ov-note">{t.emptyBody}</p>
        </section>
      ) : (
        <>
          {heroes.length > 0 && (
            <section className="card">
              <div className="ov-sec-title"><h3>{t.heroes}</h3></div>
              <div className="ov-heroes">
                {heroes.map((hero) => (
                  <div className="ov-hcard" key={hero.hero}>
                    <HeroIcon hero={hero.hero} className="ov-hcard-img" />
                    <div className="ov-hcard-b">
                      <div className="ov-hcard-n" title={hero.name}>{hero.name}</div>
                      <div className="ov-hcard-m">
                        <span>{hero.games}</span>
                        <span className={hero.tone} title={t.heroRecordTitle}>{hero.record}</span>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          )}
          <section className="card">
            <div className="ov-sec-title">
              <h3>{t.recent}</h3>
              {h.entries.length > RECENT_ROWS && (
                <button type="button" className="quiet" onClick={() => setShowAll((v) => !v)} aria-expanded={showAll}>
                  {showAll ? t.showLess : t.showAll(h.entries.length)}
                </button>
              )}
            </div>
            <div className="ov-table-wrap">
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
                      <td><span className="ov-src" title={t.sourceTitle}>Kohbor</span></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
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
          </section>
        </>
      )}
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
