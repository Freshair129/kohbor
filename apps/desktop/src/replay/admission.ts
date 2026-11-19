import type { RuntimeStatus } from "../types";
import type { SteamStatus } from "../steam/model";

export function canOpenPostMatchReplay(
  runtime: Pick<RuntimeStatus, "session" | "intake"> | null,
  steam: Pick<SteamStatus, "dota_running" | "game_phase" | "gsi" | "launch_pending"> | null,
  hostLost: boolean,
): boolean {
  if (hostLost || !steam || steam.launch_pending) return false;
  const phase = runtime?.session.phase;
  if (phase && phase !== "none" && phase !== "post_game") return false;
  if (steam.game_phase === "draft" || steam.game_phase === "pregame" || steam.game_phase === "live") return false;
  if (steam.dota_running === false) return true;
  return steam.dota_running === true && steam.game_phase === "post_game" && steam.gsi === "receiving"
    && phase === "post_game" && runtime?.intake.state === "receiving";
}
