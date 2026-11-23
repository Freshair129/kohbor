// R1: prototype tables, single match 9093797968; replaced in R4.
//
// Every table below was hand-written for the ten heroes and the 7.41 minimap of one recorded match
// in the owner's replay-viewer prototype. Another match will run, but its alerts, mobility hints
// and map projection can be incomplete or offset. The viewer shows a visible note saying so.
// R4 (design report section 8.1, U3) replaces these with the bundle manifest calibration and
// tables generated from the patch pack.

/** Liquipedia 7.41 minimap (1024 px), fitted on both fountains: px = a*x + b, py = c*y + d. */
export const MAP = { size: 1024, a: 0.049583, b: 506.7, c: -0.049108, d: 506.3 };

export const ULTS = /echo_slam|shadow_dance|amphibian_rhapsody|spirit_form|winters_curse|wall_of_replica|gods_strength|focusfire|sharpshooter$|item_black_king_bar/;
export const ULT_ABIL = /echo_slam|shadow_dance|amphibian_rhapsody|spirit_form|winters_curse|wall_of_replica|gods_strength|focusfire|sharpshooter$|fire_remnant$/;
export const CC = ["stun", "hex", "voodoo", "silence", "root", "ensnare", "bash", "sleep", "fear", "taunt", "cyclone", "frozen", "freeze", "entangle", "searing_chains", "fissure", "frogstomp", "binding", "stomp", "shackle", "bushwhack"];
export const MOBILITY = /fire_remnant|slark_pounce|shadow_dance|windrun$|scurry|spirit_form|dark_seer_surge|arctic_burn|item_blink|item_force_staff|item_tpscroll|item_travel_boots|item_glimmer_cape|item_invis_sword|item_black_king_bar|item_phase_boots/;
export const CONSUMABLE = /^item_(tango|tango_single|flask|clarity|enchanted_mango|faerie_fire|ward_observer|ward_sentry|ward_dispenser|smoke_of_deceit|dust|tpscroll|blood_grenade|famango|great_famango|bottle_refill|tome_of_knowledge|mana_draught)$/;

/** The "mid pair" roster button of the prototype match. Matches no hero in other matches. */
export const MID_PAIR = /ember_spirit|earthshaker/;

/** Opening time and death of the prototype; other matches fall back to their first death. */
export const START_T = 340;
export const DEFAULT_DEATH = { heroSuffix: "ember_spirit", from: 350, to: 360 };

/** True when the bundle's own map calibration agrees with the prototype MAP table. */
export function calibrationMatches(calibration) {
  const px = calibration && calibration.px, py = calibration && calibration.py;
  if (!Array.isArray(px) || !Array.isArray(py)) return false;
  const near = (x, y) => typeof x === "number" && Math.abs(x - y) <= Math.abs(y) * 1e-4;
  return near(px[0], MAP.a) && near(px[1], MAP.b) && near(py[0], MAP.c) && near(py[1], MAP.d);
}
