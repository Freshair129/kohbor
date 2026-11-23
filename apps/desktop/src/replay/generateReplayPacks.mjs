import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const CONTROL_TERMS = [
  "stun", "stunned", "hex", "voodoo", "silence", "silenced", "root", "ensnare", "bash", "sleep", "fear", "taunt", "taunts",
  "cyclone", "frozen", "freeze", "entangle", "searing chains", "fissure", "frogstomp", "binding",
  "stomp", "shackle", "shackles", "bushwhack", "leash", "disarm", "disarmed", "impale", "knockback", "banish",
  "binds", "cannot move", "cannot attack", "cannot cast", "immobilized", "airborne",
];
const MOBILITY_TERMS = ["blink", "teleport", "leap", "dash", "jump", "travel", "charge", "movement speed"];
const SOURCE_FILES = ["pack.json", "heroes.json", "abilities.json", "curated.json", "items.json"];
const ENGINE_COMMIT = "a609ecc9d5221dcf62acd9c0312ce7cb8d8f79f9";

const json = async file => JSON.parse(await readFile(file, "utf8"));
const sha256 = bytes => createHash("sha256").update(bytes).digest("hex");
const sorted = values => [...values].sort((a, b) => a.localeCompare(b));
const slug = heroId => heroId.replace(/^npc_dota_hero_/, "");
const flatText = ability => [ability.dname, ability.desc, ability.lore,
  ...(Array.isArray(ability.attrib) ? ability.attrib.flatMap(row => [row.key, row.header]) : [])]
  .filter(value => typeof value === "string").join(" ").toLowerCase();

function termMatches(text, term) {
  return new RegExp(`(?:^|[^a-z])${term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:$|[^a-z])`, "i").test(text);
}

export function deriveReplayData({ pack, heroes, abilities, curated, items }) {
  const heroRows = Object.values(heroes).filter(hero => typeof hero.name === "string");
  const heroNames = Object.fromEntries(heroRows
    .map(hero => [slug(hero.name), typeof hero.localized_name === "string" ? hero.localized_name : slug(hero.name)])
    .sort(([left], [right]) => left.localeCompare(right)));
  const abilityNamesByHero = Object.fromEntries(heroRows
    .map(hero => [slug(hero.name), sorted(Object.keys(abilities).filter(name => name.startsWith(`${slug(hero.name)}_`)))])
    .sort(([left], [right]) => left.localeCompare(right)));

  const controlAbilitiesByHero = {};
  const controlAbilitySource = {};
  const controlTerms = new Set();
  const mobilityAbilitiesByHero = {};
  for (const [heroSlug, names] of Object.entries(abilityNamesByHero)) {
    const control = [];
    const mobility = [];
    for (const name of names) {
      const ability = abilities[name];
      const text = flatText(ability);
      const hits = CONTROL_TERMS.filter(term => termMatches(text, term));
      for (const term of hits) controlTerms.add(term);
      if (hits.length) {
        control.push(name);
        controlAbilitySource[name] = {
          dname: typeof ability.dname === "string" ? ability.dname : null,
          desc: typeof ability.desc === "string" ? ability.desc : "",
        };
      }
      if (MOBILITY_TERMS.some(term => termMatches(text, term))) mobility.push(name);
    }
    controlAbilitiesByHero[heroSlug] = control;
    mobilityAbilitiesByHero[heroSlug] = mobility;
  }

  const consumableItems = sorted(Object.entries(items)
    .filter(([, item]) => String(item.qual ?? "").toLowerCase() === "consumable")
    .map(([name]) => name.startsWith("item_") ? name : `item_${name}`));
  const mobilityItems = sorted(Object.entries(items)
    .filter(([, item]) => MOBILITY_TERMS.some(term => termMatches(flatText(item), term)))
    .map(([name]) => name.startsWith("item_") ? name : `item_${name}`));
  const notableItems = sorted(Object.entries(items)
    .filter(([, item]) => item.dname === "Black King Bar")
    .map(([name]) => name.startsWith("item_") ? name : `item_${name}`));

  const heroBaseAttackSpeed = pack.base_attack_speed;
  if (!heroBaseAttackSpeed || typeof heroBaseAttackSpeed.default !== "number") {
    throw new Error(`${pack.patch}: pack.json is missing base_attack_speed.default`);
  }

  return {
    schema_version: 1,
    patch: pack.patch,
    source: pack.source,
    map: {
      size: pack.map.size,
      image: pack.map.image,
      calibration: pack.map.calibration,
    },
    base_attack_speed: heroBaseAttackSpeed,
    hero_names: heroNames,
    rules: pack.rules ?? {},
    formulas: pack.formulas ?? {},
    buffs: curated.buffs ?? {},
    ability_names_by_hero: abilityNamesByHero,
    control_abilities_by_hero: controlAbilitiesByHero,
    control_ability_source: controlAbilitySource,
    control_terms: sorted(controlTerms),
    mobility_abilities_by_hero: mobilityAbilitiesByHero,
    mobility_items: mobilityItems,
    consumable_items: consumableItems,
    notable_items: notableItems,
    ultimate_classification: {
      available: false,
      reason: "The supplied dotaconstants pack has no hero-to-ability slot or ultimate marker.",
      abilities_by_hero: null,
    },
  };
}

export async function generateReplayPacks(sourceRoot, outputRoot) {
  const sourceRows = [];
  const outputRows = [];
  let totalBytes = 0;

  for (const patchName of ["7.41f", "7.42"]) {
    const sourceDir = path.join(sourceRoot, patchName);
    const sources = {};
    for (const filename of SOURCE_FILES) {
      const bytes = await readFile(path.join(sourceDir, filename));
      sourceRows.push({ patch: patchName, file: filename, bytes: bytes.byteLength, sha256: sha256(bytes) });
      sources[filename] = JSON.parse(bytes.toString("utf8"));
    }
    const data = deriveReplayData({
      pack: sources["pack.json"],
      heroes: sources["heroes.json"],
      abilities: sources["abilities.json"],
      curated: sources["curated.json"],
      items: sources["items.json"],
    });
    const outputDir = path.join(outputRoot, patchName);
    await mkdir(outputDir, { recursive: true });
    const dataBytes = Buffer.from(`${JSON.stringify(data, null, 2)}\n`);
    await writeFile(path.join(outputDir, "replay-data.json"), dataBytes);
    outputRows.push({ patch: patchName, file: "replay-data.json", bytes: dataBytes.byteLength, sha256: sha256(dataBytes) });
    totalBytes += dataBytes.byteLength;

    const provenance = {
      schema_version: 1,
      patch: patchName,
      generated_from: "replay-engine-rs/pack/<patch>/",
      engine_source_commit: ENGINE_COMMIT,
      dotaconstants_version: sources["pack.json"].source,
      license_note: "The upstream dotaconstants data is identified as MIT-licensed. This provenance record is not an independent rights review; public-release rights review remains required.",
      third_party_data_entry: {
        kind: "dotaconstants-derived-replay-viewer-data",
        use: "Lab replay view: patch map calibration, hero attack speed, curated buffs, control and movement ability lookup.",
        upstream_license: "MIT (upstream notice; not independently reviewed)",
        network_fetch: false,
      },
      source_files: sourceRows.filter(row => row.patch === patchName),
      generated_files: [outputRows.at(-1)],
    };
    const provenanceBytes = Buffer.from(`${JSON.stringify(provenance, null, 2)}\n`);
    await writeFile(path.join(outputDir, "provenance.json"), provenanceBytes);
    outputRows.push({ patch: patchName, file: "provenance.json", bytes: provenanceBytes.byteLength, sha256: sha256(provenanceBytes) });
    totalBytes += provenanceBytes.byteLength;
  }

  if (totalBytes >= 2 * 1024 * 1024) throw new Error(`Replay pack data is ${totalBytes} bytes (limit is under 2 MiB)`);
  return { totalBytes, sourceRows, outputRows };
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const [, , sourceRoot, outputRoot] = process.argv;
  if (!sourceRoot || !outputRoot) throw new Error("Usage: node generateReplayPacks.mjs <source-pack-root> <output-root>");
  const summary = await generateReplayPacks(sourceRoot, outputRoot);
  process.stdout.write(`${JSON.stringify(summary, null, 2)}\n`);
}
