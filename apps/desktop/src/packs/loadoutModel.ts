export const MAX_EQUIPPED_PACKS = 4;

export interface EquippedPack {
  install_id: string;
  package_id: string;
  revision: number;
  container_sha256: string;
}

export interface PackLoadout {
  generation: number;
  equipped: EquippedPack[];
  event_locks: Record<string, string>;
  match_consistency: boolean;
}

export interface PackLoadoutState {
  schema_version: "1.0.0";
  loadout: PackLoadout;
  pending: PackLoadout | null;
  pending_request_id: string | null;
  max_equipped: number;
}

function object(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function count(value: unknown, min: number, max: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= min && value <= max;
}

function parseLoadout(value: unknown): PackLoadout | null {
  const row = object(value);
  if (!row || Object.keys(row).length !== 4 || !["generation", "equipped", "event_locks", "match_consistency"].every(key => Object.hasOwn(row, key))
    || !count(row.generation, 0, Number.MAX_SAFE_INTEGER) || !Array.isArray(row.equipped) || row.equipped.length > MAX_EQUIPPED_PACKS
    || typeof row.match_consistency !== "boolean") return null;
  const equipped = row.equipped.map(item => {
    const pack = object(item);
    if (!pack || Object.keys(pack).length !== 4 || !["install_id", "package_id", "revision", "container_sha256"].every(key => Object.hasOwn(pack, key))
      || typeof pack.install_id !== "string" || !/^ins_[0-9]{10}$/.test(pack.install_id)
      || typeof pack.package_id !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/.test(pack.package_id)
      || !count(pack.revision, 1, Number.MAX_SAFE_INTEGER)
      || typeof pack.container_sha256 !== "string" || !/^[a-f0-9]{64}$/.test(pack.container_sha256)) return null;
    return pack as unknown as EquippedPack;
  });
  if (equipped.some(pack => pack === null) || new Set(equipped.map(pack => pack!.install_id)).size !== equipped.length
    || new Set(equipped.map(pack => pack!.package_id)).size !== equipped.length) return null;
  const locks = object(row.event_locks);
  if (!locks || Object.entries(locks).some(([event, packageId]) => !/^[a-z][a-z0-9_]{0,79}$/.test(event)
    || typeof packageId !== "string" || !/^[a-z0-9][a-z0-9_.-]{0,79}$/.test(packageId))) return null;
  return { generation: row.generation, equipped: equipped as EquippedPack[], event_locks: locks as Record<string, string>, match_consistency: row.match_consistency };
}

export function parsePackLoadout(value: unknown): PackLoadoutState | null {
  const row = object(value);
  if (!row || Object.keys(row).length !== 5 || row.schema_version !== "1.0.0"
    || !Object.hasOwn(row, "loadout") || !Object.hasOwn(row, "pending") || !Object.hasOwn(row, "pending_request_id") || !Object.hasOwn(row, "max_equipped")
    || !count(row.max_equipped, 1, MAX_EQUIPPED_PACKS)) return null;
  const loadout = parseLoadout(row.loadout);
  const pending = row.pending === null ? null : parseLoadout(row.pending);
  if (!loadout || (row.pending !== null && !pending)
    || (row.pending_request_id !== null && (typeof row.pending_request_id !== "string" || !/^[A-Za-z0-9_-]{8,96}$/.test(row.pending_request_id)))) return null;
  return { schema_version: "1.0.0", loadout, pending, pending_request_id: row.pending_request_id as string | null, max_equipped: row.max_equipped };
}

export function toggleEquipped(loadout: PackLoadout, pack: { installId: string; packageId: string }): string[] | null {
  const current = loadout.equipped.map(item => item.install_id);
  const position = current.indexOf(pack.installId);
  if (position >= 0) return current.filter(id => id !== pack.installId);
  if (current.length >= MAX_EQUIPPED_PACKS || loadout.equipped.some(item => item.package_id === pack.packageId)) return null;
  return [...current, pack.installId];
}

export function moveEquipped(loadout: PackLoadout, installId: string, delta: -1 | 1): string[] | null {
  const ids = loadout.equipped.map(item => item.install_id);
  const current = ids.indexOf(installId);
  const next = current + delta;
  if (current < 0 || next < 0 || next >= ids.length) return null;
  [ids[current], ids[next]] = [ids[next]!, ids[current]!];
  return ids;
}

export function loadoutRequest(requestId: string, installIds: string[], loadout: PackLoadout, locks = loadout.event_locks, matchConsistency = loadout.match_consistency) {
  return {
    schema_version: "1.0.0",
    request_id: requestId,
    install_ids: installIds,
    event_locks: locks,
    match_consistency: matchConsistency,
    policy: "after_match",
  } as const;
}
