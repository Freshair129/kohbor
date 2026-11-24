export const HERO_CARD_ROW_IDS = Object.freeze(["top", "bars", "ab", "it", "st", "dv"]);

const unknown = value => value == null || value === "—" || value === "— ไม่ทราบ" || value === "unknown"
  || (typeof value === "number" && !Number.isFinite(value));

/** Canonical six-row presentation model shared by the replay string renderer and Live React cards. */
export function heroCardView(input = {}) {
  const rows = HERO_CARD_ROW_IDS.map(id => {
    const values = input[id] && typeof input[id] === "object" && !Array.isArray(input[id]) ? input[id] : {};
    return {
      id,
      values,
      unknownFields: Object.keys(values).filter(key => unknown(values[key])),
    };
  });
  return { rows };
}
