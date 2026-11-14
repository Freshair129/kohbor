export const DEVELOPER_MODE_KEY = "kohbor.developerMode";
// Prototype default. Production builds flip this single constant to false.
export const DEVELOPER_MODE_DEFAULT = true;

export function readDeveloperMode(storage: Pick<Storage, "getItem"> | null): boolean {
  try {
    const value = storage?.getItem(DEVELOPER_MODE_KEY);
    return value === "false" ? false : value === "true" ? true : DEVELOPER_MODE_DEFAULT;
  } catch { return DEVELOPER_MODE_DEFAULT; }
}

export function writeDeveloperMode(storage: Pick<Storage, "setItem"> | null, enabled: boolean): boolean {
  try {
    if (!storage) return false;
    storage.setItem(DEVELOPER_MODE_KEY, String(enabled));
    return true;
  } catch { return false; }
}
