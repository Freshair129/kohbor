import type { DraftBarFrame } from "../deck/draftBarModel";
import { createDraftRosterMemory, rememberDraftFrame, type DraftRosterMemory } from "./secondMonitorModel";

// Draft identities live in memory for the match only (UI-012); a new or unknown session epoch clears them.
// One memory for the Live page, shared by the centred deck and the second-monitor deck, so switching between them
// mid-session never loses or duplicates an identification.
let rosterMemory: DraftRosterMemory = createDraftRosterMemory();

export function useRosterMemory(epoch: number | null, frame: DraftBarFrame | null): DraftRosterMemory {
  rosterMemory = rememberDraftFrame(rosterMemory, epoch, frame);
  return rosterMemory;
}
