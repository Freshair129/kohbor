# Own hero derived stats

## Purpose

The Live hero card can use the own hero ID, level, and inventory supplied by
GSI together with local game scripts. GSI does not provide the hero's
attributes, so Kohbor derives these values and labels them **DERIVED** and
**without temporary effects**. The shared pure functions live in
apps/desktop/src/stats/derivedStats.ts.

## Game data

apps/desktop/src/assets/dota/6989/stats.json was extracted read-only from the
installed Dota 2 build 6989. The build number is known; the patch is
**unverified** because the inspected game files contain no explicit 7.41f or
7.42 version string. The JSON provenance lists every source script and its
physical VPK path and SHA-256, plus hashes for the VPK files and the exporter
source commit.

Hero records combine scripts/npc/heroes/npc_dota_hero_<id>.txt with
scripts/npc/heroes/npc_dota_hero_base.txt, using hero values before shared
base values. Primary attribute falls back to the matching unit in
scripts/npc/npc_heroes.txt before the shared base. The exporter records all
three sources. Fields with no parsed numeric source are null; explicit zeroes
in a script remain zero. For items, zero means the complete item script had no
mapped contribution; a recognized but unparseable or ambiguous contribution
is null. Hero AttackCapabilities is normalized to attack.type (melee/ranged)
to select attack-type-specific boot movement speed.

Item records normalize scalar entries from AbilityValues and attrib.
AbilityValues are preferred when the same key is present in both places.
Known aliases map to the following formula inputs:

| Formula input | Recognized script keys and labels |
| --- | --- |
| attributes.strength | strength, bonus_strength, bonus_str, str |
| attributes.agility | agility, bonus_agility, bonus_agi, agi |
| attributes.intelligence | intellect, intelligence, bonus_intellect, bonus_intelligence, bonus_int, int |
| attributes.all | all_stats, bonus_all_stats, bonus_stats, all_attributes, bonus_all_attributes |
| effects.armor | armor, bonus_armor, armor_bonus, armor_aura, aura_armor |
| effects.attackSpeed | attack_speed, bonus_attack_speed, attack_speed_bonus |
| effects.damage | damage, bonus_damage, damage_bonus, attack_damage |
| effects.moveSpeedFlat | bonus_movement_speed, bonus_move_speed, bonus_movement, movement_speed, move_speed |
| effects.moveSpeedPercent | bonus_movement_speed_pct, movement_speed_pct, move_speed_pct, movement_speed_percent_bonus, movespeed_pct |
| effects.moveSpeedBoots | bonus_movement_speed on recognized boots |
| effects.moveSpeedBootsMelee | bonus_movement_speed_melee |
| effects.moveSpeedBootsRanged | bonus_movement_speed_ranged |
| effects.magicResistance | bonus_magical_armor, bonus_magic_resistance, magic_resistance, magic_resist, bonus_spell_resist |
| effects.evasion | evasion, bonus_evasion |
| effects.statusResistance | status_resistance, bonus_status_resistance |
| effects.spellAmp | spell_amp, bonus_spell_amp |
| effects.lifesteal | lifesteal, bonus_lifesteal, lifesteal_percent, attack_lifesteal, lifesteal_aura |
| effects.spellLifesteal | spell_lifesteal, bonus_spell_lifesteal |
| effects.healthRegen | health_regen, bonus_health_regen, bonus_hp_regen, hp_regen, bonus_regen |
| effects.manaRegen | mana_regen, bonus_mana_regen, bonus_mp_regen, mp_regen, aura_mana_regen, mana_regen_aura |

Human-readable attrib labels such as +10 Strength are normalized to the same
names. Unmapped keys and original scalar values remain under unknown. The
dynamic bonus_stat key is not guessed: its four attribute inputs are null.
Unmapped health-regen aura/amplifier values and mana-regen multipliers remain
under unknown and make the corresponding derived regeneration input null.
AttackCapabilities is normalized to melee/ranged for selecting boot speed.
Repeated boot items use the fastest applicable value; other flat movement
speed inputs are additive. A boot with different melee/ranged values and no
known hero attack capability produces null movement speed.

The local 7.41f dotaconstants snapshot has a compatible hero shape for common
base fields, but it is not a drop-in input: its item stats are attrib arrays
that need normalization. Its patch index also does not name 7.41f or 7.42, so
it does not establish this build's patch.

## Formula inputs and outputs

heroAttributes(heroId, level, items, stats, bonuses?) returns primary
attribute and STR/AGI/INT values. Each value has value, sources, and
derived=true. Attribute growth is base + gain × (level − 1), plus normalized
inventory attributes. All-attribute bonuses contribute to each attribute.

derivedStats(heroId, level, items, stats, { rules, ...bonuses }?) returns
attributes plus damage, attack range, armor, physical damage reduction, magic
resistance, attack speed, attacks per second, movement speed, health, health
regeneration, base mana, mana regeneration, evasion, status resistance, spell
amplification, and lifesteal. Every displayed stat has its own value, source
list, and derived=true marker.

Patch-sensitive constants are caller-selected using RULES_BY_PATCH; there is
no default rule because this installed build's patch is unverified. Rule
values are transcribed from
F:/gsi-log/replay-engine-rs/pack/7.41f/pack.json and
F:/gsi-log/replay-engine-rs/pack/7.42/pack.json at commit a609ecc:

| Rule | 7.41f | 7.42 |
| --- | ---: | ---: |
| HP per STR | 22 | 20 |
| HP regen per STR | 0.1 | 0.1 |
| MR percentage points per INT | 0.1 | 0.125 |
| Armor per AGI | 0.166667 | 0.166667 |
| BKB duration / magic resistance | 9 s / 60% | 9 s / 60% |
| BKB magic resistance applies against piercing abilities | no | yes |

The BKB values are exposed for patch-aware callers. BKB and other temporary
effects are not applied by derivedStats.

Other formulas are ported from replay-viewer-proto/viewer.js and
apps/desktop/src/replay/matchViewer.mjs: armor includes AGI and item armor;
physical reduction is 0.06 × armor / (1 + 0.06 × |armor|); hero base MR and
the per-INT contribution add before item MR stacks multiplicatively; attack
speed includes base speed, AGI, and item speed and is clamped to the prototype
limits; attacks per second is attackSpeed / 100 / baseAttackTime; movement
speed applies flat and percentage item bonuses plus the fastest applicable
boot value and is clamped to 100–550;
evasion and status resistance use multiplicative stacking; spell amp and
lifesteal add. Attack damage uses the script's base damage plus the primary
attribute and item damage for strength, agility, and intelligence heroes.

In expanded form: armor = base armor + AGI × armorPerAgility + item armor;
MR = 1 − (1 − (base MR + INT × mrPercentPerIntelligence) / 100) ×
Π(1 − item MR / 100); attack speed =
max(20 + base attack speed − 100, min(700, base attack speed + AGI + item
attack speed)); move speed =
clamp((base move speed + flat item move speed + fastest applicable boot speed) ×
(1 + summed item move-speed percentage / 100), 100, 550); max health =
base health + STR × hpPerStrength; and health regeneration = base health
regeneration + STR × hpRegenPerStrength + item health regeneration.

## Unknowns and limits

- Missing or invalid hero/item inputs produce null, never a fabricated zero.
- The caller must select a patch rules object. Build 6989 alone does not prove
  which rules apply.
- Talents and attribute-bonus levels are not inferred. They are included only
  when their explicit values are passed to heroAttributes or derivedStats;
  otherwise results say talents not included and
  attribute-bonus levels not included.
- Temporary buffs, debuffs, neutral-item enchantments, active item effects,
  hero ability/passive effects, current HP/MP, and match-state changes are not
  modeled.
- Derived max health includes script base health and STR scaling, but does not
  include separate item bonus-health effects because those are outside this
  extraction's item formula inputs.
- The replay pack supplies no mana-per-INT rule. Mana output is the hero script
  base; mana regeneration is script base plus normalized item regeneration.
- bonus_stat and any unmapped item AbilityValues remain unknown. An item with
  dynamic attribute selection makes all of that item's attribute inputs
  unknown.
- The formula for primary-attribute attack damage is not a field in the replay
  pack; its use is an implementation rule that should be rechecked against
  entity data when the game rules change.
- Universal heroes retain primaryAttribute="universal" and their base damage,
  but derived total damage is null until a cited universal damage factor is
  added. No 0.6-per-attribute constant is assumed here.
- The reference checks use replay entity attributes rounded to integers with
  tolerance ±0.5. Derived tick numbers are computed from sampleT and dtick
  and are labeled derived in the tests. The reference match is 7.41f/build
  6976, while the installed data is build 6989 with an unverified patch.
  Level-15 reference fixtures therefore pass an explicit one-level +2
  attribute bonus inferred from their entity residuals; the Largo fixture
  also uses 7.41f Meteor Hammer Intelligence (+24) from the local replay
  pack's dotaconstants because build 6989 has +22. These are test-only inputs,
  not defaults inferred by production code or proof of cross-patch parity.
- The bundled file contains Valve-derived game data. A rights review is
  required before public release.
