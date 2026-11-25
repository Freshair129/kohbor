export const HERO_CARD_ROW_IDS = Object.freeze(["top", "bars", "ab", "it", "st", "dv"]);

const unknown = value => value == null || value === "—" || value === "— ไม่ทราบ" || value === "unknown"
  || (typeof value === "number" && !Number.isFinite(value));

/** Optional replay/Analytics-only seventh-row cell; Live omits it without an admitted HD value. */
export function heroDamageCell(value) {
  const known = typeof value === "number" && Number.isFinite(value) && value >= 0;
  return { label: "HD", value: known ? Math.round(value).toLocaleString("en-US") : "—", unknown: !known };
}

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
  const derived = rows.find(row => row.id === "dv")?.values ?? {};
  const hasHd = Object.prototype.hasOwnProperty.call(derived, "hd");
  return { rows, hd: hasHd ? heroDamageCell(derived.hd) : null };
}
