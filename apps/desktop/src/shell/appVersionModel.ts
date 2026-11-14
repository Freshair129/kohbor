export interface AppVersionInfo {
  version: string | null;
  git_commit: string | null;
  build_date: string | null;
  profile: "release" | "debug" | null;
  demo: boolean | null;
}

function text(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

/** Missing fields on older hosts remain unknown independently of fields that are present. */
export function parseAppVersion(input: unknown): AppVersionInfo | null {
  if (!input || typeof input !== "object" || Array.isArray(input)) return null;
  const value = input as Record<string, unknown>;
  return { version: text(value.version), git_commit: text(value.git_commit), build_date: text(value.build_date),
    profile: value.profile === "release" || value.profile === "debug" ? value.profile : null,
    demo: typeof value.demo === "boolean" ? value.demo : null };
}

export async function readAppVersion(command: () => Promise<unknown>): Promise<AppVersionInfo | null> {
  try { return parseAppVersion(await command()); } catch { return null; }
}

export function deriveAppVersionView(info: AppVersionInfo | null) {
  const version = info?.version ?? null;
  return {
    version: version ?? "—",
    compact: version === null ? "—" : version.startsWith("v") ? version : `v${version}`,
    commit: info?.git_commit?.slice(0, 9) ?? "—",
    buildDate: info?.build_date ?? "—",
    profile: info?.profile ?? "—",
    demoBadge: info?.demo === true ? "DEMO" : null,
  };
}
export type AppVersionView = ReturnType<typeof deriveAppVersionView>;
