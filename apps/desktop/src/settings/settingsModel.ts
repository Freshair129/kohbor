export const SETTINGS_SECTION_STORAGE_KEY = "kohbor.settings-section";

export const SETTINGS_CONTROL_IDS = [
  "locale", "steamName", "glassLevel", "developerMode", "objectiveAlerts", "mapKeyDiagnostics",
  "objectiveCardLayout", "replayStorage", "replayAutoParse", "analyticsAssistant", "showAppHotkey",
  "minimapCropPosition", "minimapCropSize", "minimapCropReset", "minimapDetectCrop", "minimapCropProposal",
  "minimapCropConfirm", "captureDisplayRate", "capturePause", "mapFeedback", "captureRetry", "captureResourceDetails",
  "mockCaptureReason", "newsFeed", "ownHistoryOpenDota", "publicHeroStats", "draftBar", "draftPosition", "draftBracket",
  "orbEnable", "orbDwell", "orbFocus", "orbMixer", "orbMusicApp", "orbMixerFocus", "orbMixerReset",
  "gsiConnection", "heroCardDisplay",
] as const;

export const SETTINGS_COMPONENT_IDS = [
  "generalPreferences", "developerPreferences", "objectiveAlerts", "objectiveCardLayout", "minimapCapture",
  "gsiConnection", "replayAnalytics", "showAppHotkey", "newsFeed", "ownHistoryOpenDota", "publicHeroStats", "orbAudio",
  "heroCardDisplay",
] as const;

export const SETTINGS_SECTIONS = [
  { id: "general", controlIds: ["locale", "steamName", "glassLevel"], componentIds: ["generalPreferences"] },
  { id: "alerts-objectives", controlIds: ["objectiveAlerts", "mapKeyDiagnostics"], componentIds: ["objectiveAlerts"] },
  { id: "overlay-layout", controlIds: ["objectiveCardLayout"], componentIds: ["objectiveCardLayout"] },
  {
    id: "minimap-capture",
    controlIds: [
      "minimapCropPosition", "minimapCropSize", "minimapCropReset", "minimapDetectCrop", "minimapCropProposal",
      "minimapCropConfirm", "captureDisplayRate", "capturePause", "mapFeedback", "captureRetry", "captureResourceDetails",
      "mockCaptureReason", "gsiConnection",
    ],
    componentIds: ["minimapCapture", "gsiConnection"],
  },
  { id: "replay-ai", controlIds: ["replayStorage", "replayAutoParse", "analyticsAssistant"], componentIds: ["replayAnalytics"] },
  { id: "hero-card", controlIds: ["heroCardDisplay"], componentIds: ["heroCardDisplay"] },
  { id: "hotkeys", controlIds: ["showAppHotkey"], componentIds: ["showAppHotkey"] },
  {
    id: "privacy-data",
    controlIds: ["newsFeed", "ownHistoryOpenDota", "publicHeroStats", "draftBar", "draftPosition", "draftBracket"],
    componentIds: ["newsFeed", "ownHistoryOpenDota", "publicHeroStats"],
  },
  { id: "developer", controlIds: ["developerMode"], componentIds: ["developerPreferences"] },
  {
    id: "audio-controls",
    controlIds: ["orbEnable", "orbDwell", "orbFocus", "orbMixer", "orbMusicApp", "orbMixerFocus", "orbMixerReset"],
    componentIds: ["orbAudio"],
  },
] as const;

export type SettingsSectionId = typeof SETTINGS_SECTIONS[number]["id"];
export const DEFAULT_SETTINGS_SECTION: SettingsSectionId = "overlay-layout";

export interface SettingsSectionStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export function isSettingsSectionId(value: unknown): value is SettingsSectionId {
  return typeof value === "string" && SETTINGS_SECTIONS.some((section) => section.id === value);
}

export function readSettingsSection(storage: Pick<SettingsSectionStorage, "getItem">): SettingsSectionId {
  try {
    const value = storage.getItem(SETTINGS_SECTION_STORAGE_KEY);
    return isSettingsSectionId(value) ? value : DEFAULT_SETTINGS_SECTION;
  } catch {
    return DEFAULT_SETTINGS_SECTION;
  }
}

export function rememberSettingsSection(section: SettingsSectionId, storage: Pick<SettingsSectionStorage, "setItem">): void {
  try {
    storage.setItem(SETTINGS_SECTION_STORAGE_KEY, section);
  } catch {
    // A blocked preference store must not prevent Settings from opening.
  }
}

export function initialSettingsSection(entryPoint: "layout" | "hero-card" | "replay-ai" | null, lastSection: unknown): SettingsSectionId {
  if (entryPoint === "layout") return "overlay-layout";
  if (entryPoint === "hero-card") return "hero-card";
  if (entryPoint === "replay-ai") return "replay-ai";
  return isSettingsSectionId(lastSection) ? lastSection : DEFAULT_SETTINGS_SECTION;
}
