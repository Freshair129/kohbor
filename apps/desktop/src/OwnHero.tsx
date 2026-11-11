import { copy, seconds, type Locale, type Phase } from "./copy";
import type { Field, OwnSnapshot } from "./types";

function known<T>(f: Field<T> | undefined): T | null {
  return f && f.value !== null && f.value !== undefined ? f.value : null;
}

/** A value, or an em dash. An invalid field says so instead of looking merely absent. */
function Value<T>({ field, unit = "", locale }: { field: Field<T> | undefined; unit?: string; locale: Locale }) {
  const v = known(field);
  if (v !== null) return <>{`${String(v)}${unit}`}</>;
  const text = copy(locale).own;
  return field?.availability === "invalid" ? (
    <span title={text.invalidValue}>{`— ${text.invalidValueShort}`}</span>
  ) : (
    <>—</>
  );
}

/** Proportion bar. Drawn only from two known values; never from a guess. */
function Bar({ percent, kind }: { percent: number | null; kind: "hp" | "mana" }) {
  return (
    <span className={`bar bar-${kind}`} aria-hidden="true">
      {percent !== null && <span className="bar-fill" style={{ width: `${Math.max(0, Math.min(100, percent))}%` }} />}
    </span>
  );
}

function headline(snap: OwnSnapshot | null, phase: Phase, sourceLive: boolean, locale: Locale): string {
  const text = copy(locale).own;
  switch (snap?.availability) {
    case "fresh":
      return text.fresh(seconds(snap.source.age_ms, locale));
    case "stale":
      return text.stale(seconds(snap.source.age_ms, locale));
    case "invalid":
      return text.invalid;
    case "unsupported":
      return text.unsupported;
    default:
      if (!sourceLive) return text.noData;
      if (phase === "draft") return text.waitingGame;
      if (phase === "post_game") return text.gameEnded;
      return text.waitingStart;
  }
}

const TEAM_TH = { radiant: "Radiant", dire: "Dire" } as const;

export function OwnHero({ snap, phase, sourceLive, locale }: { snap: OwnSnapshot | null; phase: Phase; sourceLive: boolean; locale: Locale }) {
  const text = copy(locale).own;
  const f = snap?.own?.fields;
  const stale = snap?.availability === "stale";
  const hp = known(f?.hp);
  const maxHp = known(f?.max_hp);
  const hpPercent = hp !== null && maxHp !== null && maxHp > 0 ? (hp / maxHp) * 100 : null;
  const mana = known(f?.mana_percent);
  const hero = known(f?.hero_id);
  const alive = known(f?.alive);
  const team = snap?.own?.team ?? null;

  return (
    <section className="card" aria-labelledby="own-h">
      <div className="card-head">
        <h2 id="own-h">{text.title}</h2>
        <span className={`small ${snap?.availability === "fresh" ? "" : "muted"}`}>{headline(snap, phase, sourceLive, locale)}</span>
      </div>
      <dl className={`vitals ${stale ? "is-stale" : ""}`}>
        <div>
            <dt>{text.hero}</dt>
          <dd>
              <span className="vital-value">{hero !== null ? hero.replace("npc_dota_hero_", "").replace(/_/g, " ") : <Value field={f?.hero_id} locale={locale} />}</span>
            {team && <span className="vital-note">{text.team} {TEAM_TH[team]}</span>}
          </dd>
        </div>
        <div>
          <dt>{text.level}</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.level} locale={locale} />
            </span>
          </dd>
        </div>
        <div>
          <dt>HP</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.hp} locale={locale} /> / <Value field={f?.max_hp} locale={locale} />
            </span>
            <Bar percent={hpPercent} kind="hp" />
          </dd>
        </div>
        <div>
          <dt>{text.mana}</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.mana_percent} unit="%" locale={locale} />
            </span>
            <Bar percent={mana} kind="mana" />
          </dd>
        </div>
        <div>
          <dt>{text.state}</dt>
          <dd>
            <span className="vital-value">{alive === true ? text.alive : alive === false ? text.dead : <Value field={f?.alive} locale={locale} />}</span>
            {alive === false && (
              <span className="vital-note">
                {text.respawnPrefix} <Value field={f?.respawn_seconds} locale={locale} /> {text.secondsUnit}
              </span>
            )}
          </dd>
        </div>
      </dl>
    </section>
  );
}
