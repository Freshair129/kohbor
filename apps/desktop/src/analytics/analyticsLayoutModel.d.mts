export type AnalyticsSizeClass = "xl" | "l" | "m" | "s";
export type AnalyticsDensity = "full" | "compact" | "roster";
export interface PanePreferences {
  cardWidth: number;
  sideWidth: number;
  split: number;
  collapsed: { radiant: boolean; dire: boolean; side: boolean; summary: boolean; assistant: boolean };
}
export interface SelectionQuote {
  text: string;
  source: string;
  time: number | null;
  action: "ask";
  changesTab: false;
  noteCount: 1;
}
export const ANALYTICS_PANES_KEY: string;
export function analyticsSizeClass(width: number): AnalyticsSizeClass;
export function analyticsPageDimensions(input: { windowWidth: number; windowHeight: number; scale?: number }): { width: number; height: number };
export function defaultPanePreferences(sizeClass: AnalyticsSizeClass, width?: number): PanePreferences;
export function readPanePreferences(storage: Storage | undefined, sizeClass: AnalyticsSizeClass, width?: number): PanePreferences;
export function writePanePreferences(storage: Storage | undefined, sizeClass: AnalyticsSizeClass, value: PanePreferences): void;
export function resetPanePreferences(storage: Storage | undefined, sizeClass: AnalyticsSizeClass, width?: number): void;
export function toggleCardColumns(collapsed: { radiant: boolean; dire: boolean }): { radiant: boolean; dire: boolean };
export function analyticsPaneLayout(input: { width: number; height: number; preferences?: PanePreferences | null }): {
  sizeClass: AnalyticsSizeClass; cardWidth: number; cardWidths: { radiant: number; dire: number }; sideWidth: number; split: number;
  collapsed: PanePreferences["collapsed"]; centreWidth: number; mapSide: number;
  density: AnalyticsDensity; scale: number; tooSmall: boolean;
};
export function analyticsTopBarLayout(width: number): {
  picker: string[]; hero: string; status: string; buttons: string[]; overflow: string[]; truncates: false;
};
export function statusTrayLayout<T extends { key?: string; name: string; kind: string; remaining?: number; permanent?: boolean; source?: string; sources?: string[]; sourceNames?: string[] }>(effects: T[], slotCount: number): {
  all: Array<T & { sources: string[] }>; visible: Array<T & { sources: string[] }>; hidden: Array<T & { sources: string[] }>; hiddenCount: number; overflow: string | null; dividerIndex?: number;
};
export function selectionQuote(input: { text: string; source: string; fallbackTime?: number | null }): SelectionQuote;
export function selectionPrompt(question: string, quotes: SelectionQuote[], locale?: "th" | "en"): string;
export function heroCardDensity(input: { columnWidth: number; height: number; scale?: number }): {
  density: AnalyticsDensity; scale: number; selectedRows: "all" | "popover";
};
export function analyticsShortcut(event: { ctrlKey?: boolean; altKey?: boolean; shiftKey?: boolean; metaKey?: boolean; key?: string }):
  "reset-layout" | "toggle-cards" | "toggle-side" | "toggle-assistant" | "ask-selection" | "note-selection" | null;
