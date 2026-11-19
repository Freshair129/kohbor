import { useEffect, useState } from "react";
import type { RuntimeStatus } from "../types";
import type { SteamStatus } from "../steam/model";

type Runtime = Pick<RuntimeStatus, "session" | "intake"> | null;
type Steam = Pick<SteamStatus, "dota_running" | "game_phase" | "gsi" | "launch_pending"> | null;

const IN_MATCH = new Set(["draft", "pregame", "live"]);

function inMatch(runtime: Runtime, steam: Steam): boolean {
  return IN_MATCH.has(runtime?.session.phase ?? "") || IN_MATCH.has(steam?.game_phase ?? "");
}

/** Runtime and Steam status both report a received post-game phase. */
function confirmedPostGame(runtime: Runtime, steam: Steam): boolean {
  return steam?.dota_running === true && steam.game_phase === "post_game" && steam.gsi === "receiving"
    && runtime?.session.phase === "post_game" && runtime.intake.state === "receiving";
}

/**
 * Whether the last match this app observed has ended. Dota reports `post_game` for only a few
 * seconds before returning to the menu, where the phase becomes unknown. The ended state is
 * therefore remembered until a draft, pregame or live phase, or a pending launch, is observed.
 */
export function nextMatchEnded(previous: boolean, runtime: Runtime, steam: Steam): boolean {
  if (inMatch(runtime, steam) || steam?.launch_pending) return false;
  if (confirmedPostGame(runtime, steam)) return true;
  return previous;
}

export function useMatchEnded(runtime: Runtime, steam: Steam): boolean {
  const [ended, setEnded] = useState(false);
  useEffect(() => {
    setEnded((previous) => nextMatchEnded(previous, runtime, steam));
  }, [runtime, steam]);
  return ended;
}

export function canOpenPostMatchReplay(runtime: Runtime, steam: Steam, hostLost: boolean, matchEnded = false): boolean {
  if (hostLost || !steam || steam.launch_pending) return false;
  const phase = runtime?.session.phase;
  if (phase && phase !== "none" && phase !== "post_game") return false;
  if (inMatch(runtime, steam)) return false;
  if (steam.dota_running === false) return true;
  if (steam.dota_running !== true) return false;
  // In the Dota menu after a match this app saw end, until the next draft, pregame or live phase.
  return confirmedPostGame(runtime, steam) || matchEnded;
}
