export const HERO_CARD_DISPLAY_KEY = "kohbor.hero-card.display";
export const HERO_CARD_DISPLAY_EVENT = "kohbor:hero-card-display-change";

export const HERO_CARD_GROUPS = [
  "attrs", "dmg", "reg", "arm", "mr", "amp", "as", "ms", "ev", "sr", "ls", "rates", "status",
];

export const HERO_CARD_GROUP_KEYS = Object.freeze({
  attrs: ["str", "agi", "int"], dmg: ["dmg"], reg: ["reg"], arm: ["arm"], mr: ["mr"], amp: ["amp"],
  as: ["as"], ms: ["ms"], ev: ["ev"], sr: ["sr"], ls: ["ls"], rates: ["gpm", "xpm"],
});

export const HERO_CARD_GROUP_LABELS = Object.freeze({
  attrs: "STR / AGI / INT", dmg: "พลังโจมตี", reg: "ฟื้นฟู HP / มานา", arm: "เกราะ", mr: "ต้านทานเวท",
  amp: "เพิ่มพลังเวท %", as: "ความเร็วโจมตี", ms: "ความเร็วเคลื่อนที่", ev: "หลบหลีก",
  sr: "ต้านทานสถานะ", ls: "ดูดเลือด", rates: "GPM / XPM", status: "บัฟและดีบัฟ",
});

export const DEFAULT_HERO_CARD_DISPLAY = Object.freeze({
  v: 1,
  show: Object.freeze(Object.fromEntries(HERO_CARD_GROUPS.map(group => [group, true]))),
});

export const CORE_HERO_CARD_DISPLAY = Object.freeze({
  v: 1,
  show: Object.freeze(Object.fromEntries(HERO_CARD_GROUPS.map(group => [group,
    ["attrs", "dmg", "arm", "mr", "ms", "status"].includes(group)]))),
});

export function normalizeHeroCardDisplay(value) {
  if (!value || typeof value !== "object" || !value.show || typeof value.show !== "object") return DEFAULT_HERO_CARD_DISPLAY;
  return {
    v: 1,
    show: Object.fromEntries(HERO_CARD_GROUPS.map(group => [group,
      typeof value.show[group] === "boolean" ? value.show[group] : true])),
  };
}

export function readHeroCardDisplay(storage) {
  try {
    return normalizeHeroCardDisplay(JSON.parse(storage?.getItem(HERO_CARD_DISPLAY_KEY) ?? "null"));
  } catch {
    return DEFAULT_HERO_CARD_DISPLAY;
  }
}

export function writeHeroCardDisplay(storage, value) {
  const normalized = normalizeHeroCardDisplay(value);
  try { storage?.setItem(HERO_CARD_DISPLAY_KEY, JSON.stringify(normalized)); }
  catch { /* A blocked preference store must not prevent card rendering. */ }
  if (typeof window !== "undefined" && typeof window.dispatchEvent === "function" && typeof CustomEvent === "function") {
    window.dispatchEvent(new CustomEvent(HERO_CARD_DISPLAY_EVENT, { detail: { value: normalized } }));
  }
  return normalized;
}

export function visibleStatKeys(display, { includeLiveOnly = true } = {}) {
  const normalized = normalizeHeroCardDisplay(display);
  return new Set(HERO_CARD_GROUPS.filter(group => normalized.show[group] && (includeLiveOnly || group !== "rates"))
    .flatMap(group => HERO_CARD_GROUP_KEYS[group] ?? []));
}

export function reflowHeroCardDisplay(display, density = "full") {
  const normalized = normalizeHeroCardDisplay(display);
  const visibleStats = [...visibleStatKeys(normalized)];
  const visibleDerived = ["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"]
    .filter(group => normalized.show[group]);
  const count = visibleDerived.length;
  return {
    visibleStats,
    showStatus: normalized.show.status,
    emptyRows: ["attrs", "derived"].filter(row => row === "attrs"
      ? !normalized.show.attrs && !normalized.show.dmg && !normalized.show.reg
      : count === 0),
    derivedColumns: count === 0 ? 0 : density === "compact" ? Math.min(3, count <= 4 ? count : Math.ceil(count / 2))
      : count <= 4 ? count : Math.ceil(count / 2),
    placeholderCount: 0,
  };
}

export function visibleHeroCardRows(display, density = "full", selected = false, includeLiveOnly = true) {
  const normalized = normalizeHeroCardDisplay(display);
  const rows = ["top", "bars", "ab"];
  if (density !== "roster" && !(density === "compact" && !selected)) rows.push("it");
  if (density !== "roster" && (density !== "compact" || selected)) {
    if (normalized.show.attrs || normalized.show.dmg || includeLiveOnly && normalized.show.rates) rows.push("st");
    if (["arm", "mr", "amp", "as", "ms", "ev", "sr", "ls"].some(group => normalized.show[group])) rows.push("dv");
  }
  return rows;
}

export function setHeroCardDisplayGroup(display, group, visible) {
  if (!HERO_CARD_GROUPS.includes(group)) return normalizeHeroCardDisplay(display);
  const current = normalizeHeroCardDisplay(display);
  return { v: 1, show: { ...current.show, [group]: Boolean(visible) } };
}
