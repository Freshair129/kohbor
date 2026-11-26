export const ANALYTICS_PANES_KEY = "kohbor.analytics.panes.v1";

const classes = ["xl", "l", "m", "s"];
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const finite = value => Number.isFinite(value);

export function analyticsSizeClass(width) {
  if (width >= 2000) return "xl";
  if (width >= 1500) return "l";
  if (width >= 1180) return "m";
  return "s";
}

export function analyticsPageDimensions({ windowWidth, windowHeight, scale = 1 }) {
  const factor = finite(scale) && scale > 0 ? scale : 1;
  return {
    width: Math.max(0, Math.floor(windowWidth / factor) - 148),
    height: Math.max(0, Math.floor(windowHeight / factor) - 152),
  };
}

export function defaultPanePreferences(sizeClass, width = 0) {
  const size = classes.includes(sizeClass) ? sizeClass : "m";
  return {
    cardWidth: size === "xl" ? 400 : size === "l" ? 344 : size === "m" ? (width >= 1348 ? 292 : 240) : 232,
    sideWidth: size === "xl" ? 520 : size === "l" ? 420 : size === "m" ? 340 : 300,
    split: 0.56,
    collapsed: {
      radiant: false,
      dire: false,
      side: size === "s",
      summary: false,
      assistant: false,
    },
  };
}

function validPreferences(value, sizeClass, width = 0) {
  if (!value || typeof value !== "object") return defaultPanePreferences(sizeClass, width);
  const defaults = defaultPanePreferences(sizeClass, width);
  const collapsed = value.collapsed && typeof value.collapsed === "object" ? value.collapsed : {};
  return {
    cardWidth: finite(value.cardWidth) ? clamp(value.cardWidth, 232, 420) : defaults.cardWidth,
    sideWidth: finite(value.sideWidth) ? clamp(value.sideWidth, 300, 640) : defaults.sideWidth,
    split: finite(value.split) ? clamp(value.split, 0.25, 0.75) : defaults.split,
    collapsed: Object.fromEntries(Object.keys(defaults.collapsed).map(key => [key,
      typeof collapsed[key] === "boolean" ? collapsed[key] : defaults.collapsed[key]])),
  };
}

export function readPanePreferences(storage, sizeClass, width = 0) {
  try {
    const root = JSON.parse(storage?.getItem(ANALYTICS_PANES_KEY) ?? "{}");
    return validPreferences(root?.[sizeClass], sizeClass, width);
  } catch {
    return defaultPanePreferences(sizeClass, width);
  }
}

export function writePanePreferences(storage, sizeClass, value) {
  try {
    const root = JSON.parse(storage?.getItem(ANALYTICS_PANES_KEY) ?? "{}");
    root[sizeClass] = validPreferences(value, sizeClass);
    storage?.setItem(ANALYTICS_PANES_KEY, JSON.stringify(root));
  } catch {
    // A blocked preference store must leave Analytics usable with defaults.
  }
}

export function resetPanePreferences(storage, sizeClass, width = 0) {
  writePanePreferences(storage, sizeClass, defaultPanePreferences(sizeClass, width));
}

export function toggleCardColumns(collapsed) {
  const next = !(collapsed?.radiant && collapsed?.dire);
  return { radiant: next, dire: next };
}

export function analyticsPaneLayout({ width, height, preferences = null }) {
  const sizeClass = analyticsSizeClass(width);
  const base = defaultPanePreferences(sizeClass);
  if (sizeClass === "m" && width - base.sideWidth - 2 * 292 - 24 >= 400) base.cardWidth = 292;
  const requested = validPreferences(preferences ?? base, sizeClass);
  const collapsed = { ...requested.collapsed };
  let sideWidth = collapsed.side ? 40 : requested.sideWidth;

  // First preserve a usable centre and roster strips, then reduce the card columns.
  const sideMaximum = width - 2 * 56 - 300 - 24;
  if (!collapsed.side && sideMaximum < 300) {
    collapsed.side = true;
    sideWidth = 40;
  } else if (!collapsed.side) {
    sideWidth = clamp(sideWidth, 300, Math.min(640, sideMaximum));
  }

  const cardMaximum = Math.floor((width - sideWidth - 300 - 24) / 2);
  let cardWidth = requested.cardWidth;
  if (cardMaximum < 232) {
    collapsed.radiant = true;
    collapsed.dire = true;
    cardWidth = 56;
  } else {
    cardWidth = clamp(cardWidth, 232, Math.min(420, cardMaximum));
  }
  const radiantWidth = collapsed.radiant ? 56 : cardWidth;
  const direWidth = collapsed.dire ? 56 : cardWidth;
  const centreWidth = Math.max(0, width - sideWidth - radiantWidth - direWidth - 24);
  const availableMapHeight = Math.max(0, height - 220);
  const mapSide = Math.floor(Math.min(centreWidth, availableMapHeight));
  const cardDensityWidth = collapsed.radiant && collapsed.dire ? 56 : cardWidth;
  const density = heroCardDensity({ columnWidth: cardDensityWidth, height, scale: 1.2 });
  return {
    sizeClass, cardWidth, cardWidths: { radiant: radiantWidth, dire: direWidth }, sideWidth, split: requested.split,
    collapsed, centreWidth, mapSide, density: density.density, scale: density.scale,
    tooSmall: width < 880 || height < 440 || mapSide < 300,
  };
}

export function analyticsTopBarLayout(width) {
  if (width >= 1181) return {
    picker: ["portrait", "dateTime", "hero", "result", "duration"], hero: "portraitLabel", status: "dotLabel",
    buttons: ["cards", "side", "cardSettings", "layout", "more"], overflow: [], truncates: false,
  };
  if (width >= 981) return {
    picker: ["portrait", "dateTime", "hero", "result"], hero: "portrait", status: "dot",
    buttons: ["cards", "side", "cardSettings", "layout", "more"], overflow: [], truncates: false,
  };
  if (width >= 760) return {
    picker: ["portrait", "shortDate", "hero", "result"], hero: "portrait", status: "dot",
    buttons: ["cards", "side", "cardSettings", "layout", "more"], overflow: [], truncates: false,
  };
  return {
    picker: ["portrait", "hero", "resultDot"], hero: "more", status: "dot",
    buttons: ["cards", "side", "more"], overflow: ["myHero", "cardSettings", "layout"], truncates: false,
  };
}

const effectPriority = item => item.kind === "control" ? 0
  : item.kind === "debuff" ? 1
    : item.permanent ? 3 : 2;

export function statusTrayLayout(effects, slotCount) {
  const byName = new Map();
  for (const effect of Array.isArray(effects) ? effects : []) {
    const key = String(effect.key ?? effect.name ?? "");
    if (!key) continue;
    const current = byName.get(key);
    const sources = [...new Set([...(current?.sources ?? current?.sourceNames ?? []), ...(current?.sources ?? []),
      ...(effect.sourceNames ?? []), ...(effect.sources ?? (effect.source ? [effect.source] : []))])];
    const longest = !current || (effect.remaining ?? -1) > (current.remaining ?? -1) ? effect : current;
    byName.set(key, { ...longest, sources, sourceNames: sources });
  }
  const ordered = [...byName.values()].sort((left, right) => effectPriority(left) - effectPriority(right)
    || (left.remaining ?? Number.POSITIVE_INFINITY) - (right.remaining ?? Number.POSITIVE_INFINITY)
    || String(left.name).localeCompare(String(right.name)));
  const slots = Math.max(2, Math.min(8, Math.floor(slotCount || 2)));
  const controls = ordered.filter(item => item.kind === "control");
  const others = ordered.filter(item => item.kind !== "control");
  const selected = ordered.length <= slots
    ? new Set(ordered)
    : new Set([...controls, ...others.slice(0, Math.max(0, slots - controls.length - 1))]);
  const hidden = ordered.filter(item => !selected.has(item));
  const visibleBuffs = [...selected].filter(item => item.kind === "buff")
    .sort((left, right) => (left.permanent ? 1 : 0) - (right.permanent ? 1 : 0)
      || (left.remaining ?? Number.POSITIVE_INFINITY) - (right.remaining ?? Number.POSITIVE_INFINITY));
  // Show buffs first and put the shortest remaining debuffs at the right edge.
  const visibleDebuffs = [...selected].filter(item => item.kind !== "buff")
    .sort((left, right) => (right.remaining ?? Number.POSITIVE_INFINITY) - (left.remaining ?? Number.POSITIVE_INFINITY));
  const visible = [...visibleBuffs, ...visibleDebuffs];
  return { all: ordered, visible, hidden, hiddenCount: hidden.length, overflow: hidden.length ? "+" + hidden.length : null, dividerIndex: visibleBuffs.length };
}

export function selectionQuote({ text, source, fallbackTime = null }) {
  const clean = String(text ?? "").replace(/\s+/g, " ").trim().slice(0, 600);
  const match = clean.match(/\b(\d{1,2}):(\d{2})\b/);
  const time = match ? Number(match[1]) * 60 + Number(match[2]) : finite(fallbackTime) ? fallbackTime : null;
  return { text: clean, source: String(source ?? ""), time, action: "ask", changesTab: false, noteCount: 1 };
}

export function selectionPrompt(question, quotes, locale = "th") {
  const entries = Array.isArray(quotes) ? quotes.slice(-3) : [];
  const quoted = entries.map(quote => {
    const time = quote.time === null || !Number.isFinite(quote.time) ? ""
      : " · " + Math.floor(quote.time / 60) + ":" + String(Math.floor(quote.time % 60)).padStart(2, "0");
    return "> " + String(quote.text ?? "") + "\n— " + String(quote.source ?? "") + time;
  });
  const prompt = String(question ?? "").trim() || (locale === "en" ? "Explain this selected text." : "ช่วยอธิบายข้อความนี้");
  return [...quoted, prompt].filter(Boolean).join("\n\n");
}

export function heroCardDensity({ columnWidth, height, scale = 1 }) {
  const cardWidth = Math.max(56, Number(columnWidth) || 56);
  const cardHeight = Math.max(0, Number(height) || 0);
  const fullScale = Math.min(cardWidth / 344, Math.max(0, cardHeight - 20) / 870, scale, 1.2);
  if (cardWidth >= 292 && fullScale >= 0.85) {
    return { density: "full", scale: clamp(fullScale, 0.85, 1.2), selectedRows: "all" };
  }
  const compactScale = Math.min(cardWidth / 240, cardHeight / 671, scale, 1.1);
  if (cardWidth >= 232 && compactScale >= 0.85) {
    return { density: "compact", scale: clamp(compactScale, 0.85, 1.1), selectedRows: "all" };
  }
  return { density: "roster", scale: 1, selectedRows: "popover" };
}

export function analyticsShortcut(event) {
  if (!event?.ctrlKey || event.metaKey) return null;
  const key = String(event.key ?? "").toLowerCase();
  if (event.altKey) return key === "0" && !event.shiftKey ? "reset-layout" : null;
  if (key === "\\" && event.shiftKey) return "toggle-cards";
  if (key === "\\" && !event.shiftKey) return "toggle-side";
  if (key === "j" && !event.shiftKey) return "toggle-assistant";
  if (event.shiftKey && key === "a") return "ask-selection";
  if (event.shiftKey && key === "n") return "note-selection";
  return null;
}
