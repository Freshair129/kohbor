export interface AbilityName { name: string; rawId: string }
/** Runtime supplies the actual GSI hero id, including multiword prefixes. Without
 * hero context only the first token is stripped; this display heuristic is UNVERIFIED. */
export function displayAbilityName(abilityId: string | null, heroId?: string | null): AbilityName | null {
  if (!abilityId?.trim()) return null;
  const heroPrefix = heroId?.replace(/^npc_dota_hero_/, "");
  const words = heroPrefix ? abilityId.startsWith(`${heroPrefix}_`) ? abilityId.slice(heroPrefix.length + 1) : abilityId
    : abilityId.includes("_") ? abilityId.slice(abilityId.indexOf("_") + 1) : abilityId;
  const name = words.split("_").filter(Boolean).map(word => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()).join(" ");
  return { name: name || abilityId, rawId: abilityId };
}
