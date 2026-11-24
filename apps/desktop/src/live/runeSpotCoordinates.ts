/**
 * Positions come from rune pickup events in three local replay bundles.
 * Coordinates are projected with the same manifest calibration and map size
 * used by the replay viewer. Bundle metadata did not include a Dota build tag.
 */
export const RUNE_SPOT_PROVENANCE = {
  source: "local replay bundle world.json runes events",
  replayBundles: 3,
  patch: "7.41f",
  gameBuild: null,
  engineVersion: "0.2.0",
  eventSelection: "Bounty pickup events within the first second; standard power-rune pickups across replay times starting at 6:00",
  calibrationSource: "replay manifest map.calibration",
  mapSize: 1024,
  transform: {
    x: "(0.049583 * worldX + 506.7) / 1024",
    y: "(-0.049108 * worldY + 506.3) / 1024",
  },
  buildNote: "The replay manifest and bundle files do not record a Dota build tag.",
} as const;

type RuneSpot = {
  x: number;
  y: number;
  world: { x: number; y: number };
  source: "local replay rune event + replay manifest calibration";
  patch: "7.41f";
  build: null;
  replayBundles: 3;
};

function project(worldX: number, worldY: number): RuneSpot {
  return {
    x: (0.049583 * worldX + 506.7) / RUNE_SPOT_PROVENANCE.mapSize,
    y: (-0.049108 * worldY + 506.3) / RUNE_SPOT_PROVENANCE.mapSize,
    world: { x: worldX, y: worldY },
    source: "local replay rune event + replay manifest calibration",
    patch: RUNE_SPOT_PROVENANCE.patch,
    build: RUNE_SPOT_PROVENANCE.gameBuild,
    replayBundles: RUNE_SPOT_PROVENANCE.replayBundles,
  };
}

export const RUNE_SPOTS = {
  power_rune: [project(-1640, 1112), project(1180, -1216)],
  bounty_rune: [project(-1640, 1112), project(-996, 4431), project(595, -4660), project(1180, -1216)],
  wisdom_rune: [],
  lotus: [],
} as const;
