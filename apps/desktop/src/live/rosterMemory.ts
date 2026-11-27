import type { DraftBarFrame } from "../deck/draftBarModel";
import { createDraftRosterMemory, rememberDraftFrame, type DraftRosterMemory } from "./secondMonitorModel";
import { liveMockRequested } from "./mockMode";

// Draft identities live in memory for the match only (UI-012); a new or unknown session epoch clears them.
// One memory for the Live page, shared by the centred deck and the second-monitor deck, so switching between them
// mid-session never loses or duplicates an identification.
let rosterMemory: DraftRosterMemory = createDraftRosterMemory();
// Keep an explicitly seeded dev roster intact while the host status poll catches up to its epoch.
// Without this, a render on the previous phase can clear the seed before the Debrief renders.
let harnessRosterMemory: DraftRosterMemory | null = null;

/** Seed roster memory only for the explicit development harness; never a production data source. */
export function seedSyntheticDraftRosterForHarness(epoch: number | null, slots: readonly (string | null)[] = []) {
  if (!liveMockRequested()) return;
  harnessRosterMemory = epoch === null
    ? null
    : { epoch, slots: Array.from({ length: 10 }, (_, index) => slots[index] ?? null) };
  rosterMemory = harnessRosterMemory ?? createDraftRosterMemory();
}

export function useRosterMemory(epoch: number | null, frame: DraftBarFrame | null): DraftRosterMemory {
  if (harnessRosterMemory) {
    if (epoch === harnessRosterMemory.epoch) {
      harnessRosterMemory = rememberDraftFrame(harnessRosterMemory, epoch, frame);
      rosterMemory = harnessRosterMemory;
    }
    return harnessRosterMemory;
  }
  rosterMemory = rememberDraftFrame(rosterMemory, epoch, frame);
  return rosterMemory;
}
