const overrides = Object.freeze({
  modifier_eul_cyclone: { kind: "item", icon: "cyclone" },
  modifier_item_dustofappearance: { kind: "item", icon: "dust" },
  modifier_item_invisibility_edge_windwalk: { kind: "item", icon: "invis_sword" },
  modifier_tower_aura: { kind: "ability", icon: "tower_armor_aura" },
  modifier_tower_aura_bonus: { kind: "ability", icon: "tower_armor_aura" },
  modifier_fountain_aura: { kind: "ability", icon: "fountain_heal" },
  modifier_fountain_aura_buff: { kind: "ability", icon: "fountain_heal" },
});

const categoryGlyphs = [
  ["stun", "✹"], ["bash", "✹"], ["hex", "♜"], ["silence", "⊘"], ["root", "⌁"], ["knockback", "↗"],
  ["ensnare", "⌁"], ["slow", "◷"], ["disarm", "⚔"], ["break", "⨯"],
  ["invisible", "◌"], ["invis", "◌"], ["heal", "+"], ["shield", "⬡"],
  ["damage_up", "↗"], ["armor_down", "⌄"],
];

const tokenPrefixes = value => {
  const tokens = value.split("_").filter(Boolean);
  return tokens.map((_, index) => tokens.slice(0, tokens.length - index).join("_"));
};
const compact = value => String(value ?? "").toLowerCase().replace(/^modifier_/, "").replaceAll("_", "");

function iconResult(kind, icon, tier, inferred = false, sourceHeroId = null) {
  return { kind, icon, glyph: null, tier, inferred, sourceHeroId };
}

export function resolveModifierIcon({
  modifier, sourceHeroId = null, sourceAbilities = [], recentCasts = [],
  iconExists = () => false, overrideTable = {}, effectKind = "buff",
}) {
  const name = String(modifier ?? "").toLowerCase();
  const override = overrideTable[name] ?? overrides[name];
  if (override && typeof override.icon === "string" && iconExists(override.kind, override.icon)) {
    return iconResult(override.kind, override.icon, 0, false, sourceHeroId);
  }

  if (name.startsWith("modifier_item_")) {
    for (const candidate of tokenPrefixes(name.slice("modifier_item_".length))) {
      if (iconExists("item", candidate)) return iconResult("item", candidate, 1, false, sourceHeroId);
    }
  }

  const base = name.replace(/^modifier_/, "");
  for (const kind of ["ability", "item"]) {
    for (const candidate of tokenPrefixes(base)) {
      if (iconExists(kind, candidate)) return iconResult(kind, candidate, 2, false, sourceHeroId);
    }
  }

  const ability = sourceAbilities.find(value => compact(value) === compact(base));
  if (ability && iconExists("ability", ability)) return iconResult("ability", ability, 3, false, sourceHeroId);

  const recent = [...recentCasts].filter(cast => Number.isFinite(cast?.time)
    && Number.isFinite(cast?.start) && cast.time <= cast.start && cast.start - cast.time <= 3)
    .sort((left, right) => right.time - left.time)[0];
  if (recent && typeof recent.name === "string" && iconExists("ability", recent.name)) {
    return iconResult("ability", recent.name, 4, true, sourceHeroId);
  }

  const category = categoryGlyphs.find(([term]) => name.includes(term))?.[1];
  if (category) return { kind: "glyph", icon: null, glyph: category, tier: 5, inferred: true, sourceHeroId };
  return { kind: "glyph", icon: null, glyph: effectKind === "debuff" || effectKind === "control" ? "▼" : "▲",
    tier: 6, inferred: false, sourceHeroId };
}

export function statusIconTitle({ name, kind = "buff", sourceNames = [], remaining, permanent = false, stacks = null, inferred = false, locale = "th" }) {
  const source = sourceNames.length ? sourceNames.join(", ") : locale === "en" ? "Unknown source" : "ไม่ทราบที่มา";
  const safeRemaining = Number.isFinite(remaining) ? Math.max(0, remaining) : null;
  const duration = permanent ? "∞"
    : safeRemaining === null ? (locale === "en" ? "Time unknown" : "ไม่ทราบเวลา")
    : safeRemaining < 10 ? `${safeRemaining.toFixed(1)} ${locale === "en" ? "seconds" : "วินาที"}`
      : safeRemaining < 60 ? `${Math.ceil(safeRemaining)} ${locale === "en" ? "seconds" : "วินาที"}`
        : `${Math.floor(safeRemaining / 60)}m`;
  const className = locale === "en" ? kind === "control" ? "control" : kind === "debuff" ? "debuff" : "buff"
    : kind === "control" ? "ควบคุม" : kind === "debuff" ? "ดีบัฟ" : "บัฟ";
  const stackCount = Number.isInteger(stacks) && stacks > 0
    ? ` · ${stacks} ${locale === "en" ? (stacks === 1 ? "stack" : "stacks") : "สแต็ก"}` : "";
  const inference = inferred ? (locale === "en" ? " · likely from this source" : " · น่าจะมาจากแหล่งนี้") : "";
  return `${name} · ${className} · ${source} · ${duration}${stackCount}${inference}`;
}
