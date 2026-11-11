import { seconds, type Phase } from "./copy";
import type { Field, OwnSnapshot } from "./types";

function known<T>(f: Field<T> | undefined): T | null {
  return f && f.value !== null && f.value !== undefined ? f.value : null;
}

/** A value, or an em dash. An invalid field says so instead of looking merely absent. */
function Value<T>({ field, unit = "" }: { field: Field<T> | undefined; unit?: string }) {
  const v = known(field);
  if (v !== null) return <>{`${String(v)}${unit}`}</>;
  return field?.availability === "invalid" ? <span title="ค่าจากเกมไม่ถูกต้อง">— ไม่ถูกต้อง</span> : <>—</>;
}

/** Proportion bar. Drawn only from two known values; never from a guess. */
function Bar({ percent, kind }: { percent: number | null; kind: "hp" | "mana" }) {
  return (
    <span className={`bar bar-${kind}`} aria-hidden="true">
      {percent !== null && <span className="bar-fill" style={{ width: `${Math.max(0, Math.min(100, percent))}%` }} />}
    </span>
  );
}

function headline(snap: OwnSnapshot | null, phase: Phase, sourceLive: boolean): string {
  switch (snap?.availability) {
    case "fresh":
      return `สด · ${seconds(snap.source.age_ms)}`;
    case "stale":
      return `ข้อมูลเก่า · ${seconds(snap.source.age_ms)} ที่แล้ว`;
    case "invalid":
      return "ยืนยันผู้เล่นไม่ได้ — ไม่แสดงค่า";
    case "unsupported":
      return "ไม่รองรับ — ผู้ชม รีเพลย์ หรือเกมคัสตอม";
    default:
      if (!sourceLive) return "ยังไม่มีข้อมูล";
      if (phase === "draft") return "รอเริ่มเกม — ค่าจะแสดงเมื่อเข้าเกม";
      if (phase === "post_game") return "จบเกมแล้ว";
      return "ยังไม่มีข้อมูล — รอเข้าเกม";
  }
}

const TEAM_TH = { radiant: "Radiant", dire: "Dire" } as const;

export function OwnHero({ snap, phase, sourceLive }: { snap: OwnSnapshot | null; phase: Phase; sourceLive: boolean }) {
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
        <h2 id="own-h">ฮีโร่ของคุณ</h2>
        <span className={`small ${snap?.availability === "fresh" ? "" : "muted"}`}>{headline(snap, phase, sourceLive)}</span>
      </div>
      <dl className={`vitals ${stale ? "is-stale" : ""}`}>
        <div>
          <dt>ฮีโร่</dt>
          <dd>
            <span className="vital-value">{hero !== null ? hero.replace("npc_dota_hero_", "").replace(/_/g, " ") : <Value field={f?.hero_id} />}</span>
            {team && <span className="vital-note">ทีม {TEAM_TH[team]}</span>}
          </dd>
        </div>
        <div>
          <dt>เลเวล</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.level} />
            </span>
          </dd>
        </div>
        <div>
          <dt>HP</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.hp} /> / <Value field={f?.max_hp} />
            </span>
            <Bar percent={hpPercent} kind="hp" />
          </dd>
        </div>
        <div>
          <dt>มานา</dt>
          <dd>
            <span className="vital-value mono">
              <Value field={f?.mana_percent} unit="%" />
            </span>
            <Bar percent={mana} kind="mana" />
          </dd>
        </div>
        <div>
          <dt>สถานะ</dt>
          <dd>
            <span className="vital-value">{alive === true ? "มีชีวิต" : alive === false ? "ตาย" : <Value field={f?.alive} />}</span>
            {alive === false && (
              <span className="vital-note">
                เกิดใหม่ใน <Value field={f?.respawn_seconds} /> วิ
              </span>
            )}
          </dd>
        </div>
      </dl>
    </section>
  );
}
