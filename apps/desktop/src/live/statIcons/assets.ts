import type { StatIconId } from "./catalog";

const gameIcons = import.meta.glob<string>("../../assets/dota/*/stat-icons/*.png", {
  eager: true,
  query: "?url",
  import: "default",
});

const newestByIcon = new Map<StatIconId, { build: number; url: string }>();
for (const [path, url] of Object.entries(gameIcons)) {
  const match = /dota\/(\d+)\/stat-icons\/([^/]+)\.png$/i.exec(path.replace(/\\/g, "/"));
  if (!match || typeof url !== "string" || !url) continue;
  const iconId = match[2] as StatIconId;
  const build = Number(match[1]);
  const previous = newestByIcon.get(iconId);
  if (Number.isSafeInteger(build) && (!previous || build > previous.build)) {
    newestByIcon.set(iconId, { build, url });
  }
}

const originalIcons: Partial<Record<StatIconId, string>> = {
  spell_amp: new URL("./spell_amp.svg", import.meta.url).href,
  status_resistance: new URL("./status_resistance.svg", import.meta.url).href,
  gold_coin: new URL("./gpm.svg", import.meta.url).href,
  xp: new URL("./xpm.svg", import.meta.url).href,
};

export function statIconUrl(iconId: StatIconId): string | null {
  return newestByIcon.get(iconId)?.url ?? originalIcons[iconId] ?? null;
}
