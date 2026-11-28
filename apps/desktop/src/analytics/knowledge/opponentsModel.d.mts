export interface OpponentStatsField {
  complete: boolean;
  complete_by_player_hero_id?: Record<string, boolean>;
  by_player_hero_id?: Record<string, Record<string, number> | null>;
  other_by_player_hero_id?: Record<string, number | null>;
}
export interface OpponentStats {
  kill_gold_by_victim?: OpponentStatsField;
  gold_lost_by_killer?: OpponentStatsField;
  neutral_kill_gold_by_camp?: OpponentStatsField & {
    zone_names?: Record<string, string>;
    unattached_by_player_hero_id?: Record<string, number>;
  };
}
export interface OpponentLeader {
  state: "unknown" | "none" | "known";
  names: string[];
  heroIds: string[];
  zoneIds: string[];
  count: number | null;
  gold: number | null;
}
export interface OpponentHeroRow {
  heroId: string;
  hero: string;
  kills: number | null;
  deaths: number | null;
  killGold: number | null;
  goldLost: number | null;
}
export interface OpponentsViewModel {
  state: "available" | "unknown";
  reason: "own_hero_unknown" | "own_side_unknown" | "enemy_team_unknown" | null;
  ownHeroId: string | null;
  enemyRows: OpponentHeroRow[];
  otherDeaths: number | null;
  otherGoldLost: number | null;
  unknownKillerDeaths: number | null;
  jungleUnattached: number | null;
  headlines: {
    mostKills: OpponentLeader;
    mostKillGold: OpponentLeader;
    mostDeaths: OpponentLeader;
    mostGoldLost: OpponentLeader;
    mostJungleGold: OpponentLeader;
  };
}
export function buildOpponentsViewModel(input?: {
  digest?: unknown;
  matchData?: unknown;
  ownHeroId?: string | null;
}): OpponentsViewModel;
