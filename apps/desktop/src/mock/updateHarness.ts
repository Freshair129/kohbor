import type { UpdateReply } from "../shell/updateModel";

export type MockUpdateScenario = "available" | "failed" | "up_to_date" | "downloading";

export function parseMockUpdateScenario(value: string | null): MockUpdateScenario | null {
  if (value === "available" || value === "failed" || value === "up_to_date" || value === "downloading") return value;
  return null;
}

function clone(reply: UpdateReply): UpdateReply {
  return {
    ...reply,
    available: reply.available ? { ...reply.available } : null,
  };
}

function initialReply(scenario: MockUpdateScenario | null): UpdateReply {
  const base: UpdateReply = {
    state: "not_configured",
    current_version: "0.3.0-dev.15",
    reason: "BUILD_WITHOUT_UPDATER",
    available: null,
    match_active: false,
  };
  if (scenario === "available") {
    return {
      ...base,
      state: "available",
      reason: null,
      available: { version: "0.3.1-mock", notes: "ตัวอย่างอัปเดต\nข้อความนี้เป็นข้อมูลจำลอง", pub_date: null },
    };
  }
  if (scenario === "failed") return { ...base, state: "failed", reason: "NETWORK" };
  if (scenario === "up_to_date") return { ...base, state: "up_to_date", reason: null };
  if (scenario === "downloading") return { ...base, state: "downloading", reason: null };
  return base;
}

/** Synthetic updater responses for the explicit dev preview only. */
export function createMockUpdater(scenario: MockUpdateScenario | null) {
  let current = initialReply(scenario);
  const updateStatus = async () => clone(current);
  const updateCheck = async () => {
    current = initialReply(scenario);
    return clone(current);
  };
  const updateInstall = async (args: Record<string, unknown>) => {
    if (args.confirm !== true) throw new Error("update_install requires explicit confirmation");
    if (current.state !== "available" || current.match_active) return clone(current);
    current = { ...current, state: "ready_to_install" };
    return clone(current);
  };
  return { updateStatus, updateCheck, updateInstall };
}
