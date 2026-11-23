import {
  GuestAccountPort,
  type AccountActionResult,
  type AccountPort,
  type AccountSession,
  type AccountSnapshot,
  type AccountView,
} from "./accountView";
import { countGraphemeClusters } from "./accountViewModel";

const emptySnapshot: AccountSnapshot = {
  state: "signed_out",
  serviceOpen: true,
  liveMatch: false,
  invitedOnly: false,
  displayName: null,
  friendCode: { value: null, freshness: "unknown", isExample: false },
  sessions: null,
  handoffDeadline: null,
  handoffPurpose: "sign_in",
  firstSignIn: false,
  dialog: null,
  nameError: null,
  notice: null,
  rateLimitDeadline: null,
  deleteStep: null,
  deleteError: null,
  freshSignInDeadline: null,
  codeReplacementsRemaining: 3,
  requestFailed: false,
};

/** In-memory-only fake. Imported by tests and the explicit developer preview, never by the app default. */
export class FakeAccountPort extends GuestAccountPort implements AccountPort {
  private snapshot: AccountSnapshot;
  private listeners = new Set<() => void>();
  private codeCounter = 0;
  private readonly clock: () => number;

  constructor(initial: Partial<AccountSnapshot> & { now?: () => number } = {}) {
    super();
    const { now, ...values } = initial;
    this.clock = now ?? Date.now;
    this.snapshot = { ...emptySnapshot, ...values, friendCode: values.friendCode ? { ...values.friendCode, isExample: values.friendCode.isExample ?? false } : emptySnapshot.friendCode };
  }

  override get = (): AccountView => {
    const state = this.snapshot.state;
    return {
      state: state === "signed_in_online" ? "online" : state === "signed_in_offline" ? "offline"
        : state === "session_revoked" ? "expired" : "guest",
      displayName: this.snapshot.displayName,
      avatar: null,
      canSignIn: state === "signed_out" && this.snapshot.serviceOpen && !this.snapshot.liveMatch,
      canSignOut: state === "signed_in_online" || state === "signed_in_offline",
      friendsAvailable: state === "signed_in_online" || state === "signed_in_offline",
    };
  };

  override getState = (): AccountSnapshot => this.snapshot;

  override subscribe = (cb: () => void): (() => void) => {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  };

  setScenario(values: Partial<AccountSnapshot>): void {
    this.update({ ...emptySnapshot, ...values, friendCode: values.friendCode ? { ...values.friendCode, isExample: values.friendCode.isExample ?? false } : emptySnapshot.friendCode });
  }

  async signIn(): Promise<void> {
    if (this.snapshot.state === "browser_handoff" && this.snapshot.handoffPurpose === "sign_in") {
      this.update({ ...this.snapshot, handoffDeadline: this.clock() + 5 * 60_000 });
      return;
    }
    if (this.snapshot.state !== "signed_out" || !this.snapshot.serviceOpen || this.snapshot.liveMatch) return;
    this.update({
      ...this.snapshot,
      state: "browser_handoff",
      notice: null,
      handoffPurpose: "sign_in",
      handoffDeadline: this.clock() + 5 * 60_000,
    });
  }

  async cancelHandoff(): Promise<void> {
    if (this.snapshot.state !== "browser_handoff") return;
    if (this.snapshot.handoffPurpose === "delete") {
      this.update({ ...this.snapshot, state: "deleting", handoffDeadline: null, deleteStep: "review" });
      return;
    }
    this.update({ ...this.snapshot, state: "signed_out", handoffDeadline: null, notice: "cancelled" });
  }

  async saveDisplayName(name: string): Promise<AccountActionResult> {
    if (this.snapshot.state !== "needs_display_name" && this.snapshot.state !== "signed_in_online") return { ok: false, reason: "invalid_state" };
    const clusters = countGraphemeClusters(name);
    if (clusters === 0) {
      this.update({ ...this.snapshot, nameError: "empty" });
      return { ok: false, reason: "failed" };
    }
    if (clusters > 32) {
      this.update({ ...this.snapshot, nameError: "long" });
      return { ok: false, reason: "failed" };
    }
    if (/\p{Cc}|\p{Cf}/u.test(name)) {
      this.update({ ...this.snapshot, nameError: "refused" });
      return { ok: false, reason: "failed" };
    }
    const firstSignIn = this.snapshot.state === "needs_display_name";
    this.update({ ...this.snapshot, state: "signed_in_online", displayName: name, firstSignIn, nameError: null });
    return { ok: true };
  }

  async replaceFriendCode(): Promise<AccountActionResult> {
    if (this.snapshot.state !== "signed_in_online") return { ok: false, reason: "invalid_state" };
    if (this.snapshot.liveMatch) return { ok: false, reason: "live_match" };
    if (this.snapshot.codeReplacementsRemaining === 0) return { ok: false, reason: "limit_reached" };
    this.codeCounter += 1;
    const remaining = this.snapshot.codeReplacementsRemaining === null ? null : Math.max(0, this.snapshot.codeReplacementsRemaining - 1);
    this.update({
      ...this.snapshot,
      friendCode: { value: `KB-DEMO-${String(this.codeCounter).padStart(4, "0")}`, freshness: "live", isExample: true },
      codeReplacementsRemaining: remaining,
      dialog: null,
    });
    return { ok: true };
  }

  async listSessions(): Promise<readonly AccountSession[] | null> {
    return this.snapshot.sessions;
  }

  async retryConnection(): Promise<void> {
    if (this.snapshot.state === "signed_in_offline") this.update({ ...this.snapshot, state: "signed_in_online" });
    else if (this.snapshot.state === "service_unavailable") this.update({ ...this.snapshot, state: "signed_out" });
  }

  async signOutHere(): Promise<void> {
    this.clearLocalAccountData("signed_out");
  }

  async signOutEverywhere(): Promise<void> {
    this.clearLocalAccountData("signed_out");
  }

  async exportAccount(): Promise<AccountActionResult> {
    return this.snapshot.state === "signed_in_online" ? { ok: true } : { ok: false, reason: "invalid_state" };
  }

  async beginDelete(): Promise<void> {
    if (this.snapshot.state !== "signed_in_online" || this.snapshot.liveMatch) return;
    this.update({ ...this.snapshot, state: "deleting", deleteStep: "review", deleteError: null });
  }

  async deleteWithFreshSignIn(): Promise<void> {
    if (this.snapshot.state === "browser_handoff" && this.snapshot.handoffPurpose === "delete" && !this.snapshot.liveMatch) {
      this.update({ ...this.snapshot, handoffDeadline: this.clock() + 5 * 60_000, deleteError: null });
      return;
    }
    if (this.snapshot.state !== "deleting" || (this.snapshot.deleteStep !== "review" && !this.snapshot.deleteError) || this.snapshot.liveMatch) return;
    this.update({
      ...this.snapshot,
      state: "browser_handoff",
      handoffPurpose: "delete",
      handoffDeadline: this.clock() + 5 * 60_000,
      deleteStep: "reauth",
    });
  }

  /** Developer-only event injection used to exercise the two-step fresh-sign-in flow. */
  completeFreshSignIn(matchesCurrentAccount = true): void {
    if (this.snapshot.handoffPurpose !== "delete" || this.snapshot.state !== "browser_handoff") return;
    if (this.snapshot.handoffDeadline !== null && this.clock() >= this.snapshot.handoffDeadline) {
      this.update({ ...this.snapshot, state: "deleting", deleteError: "window", handoffDeadline: null });
      return;
    }
    if (!matchesCurrentAccount) {
      this.update({ ...this.snapshot, state: "deleting", deleteError: "mismatch", handoffDeadline: null });
      return;
    }
    this.update({
      ...this.snapshot,
      state: "deleting",
      deleteStep: "confirm",
      deleteError: null,
      handoffDeadline: null,
      freshSignInDeadline: this.clock() + 5 * 60_000,
    });
  }

  async confirmDelete(): Promise<AccountActionResult> {
    if (this.snapshot.state !== "deleting" || this.snapshot.deleteStep !== "confirm") return { ok: false, reason: "invalid_state" };
    if (this.snapshot.liveMatch) return { ok: false, reason: "live_match" };
    if (this.snapshot.freshSignInDeadline === null || this.clock() >= this.snapshot.freshSignInDeadline) {
      this.update({ ...this.snapshot, deleteError: "window" });
      return { ok: false, reason: "failed" };
    }
    this.clearLocalAccountData("signed_out");
    this.update({ ...this.snapshot, state: "signed_out", notice: "deleted", deleteStep: null, deleteError: null });
    return { ok: true };
  }

  async keepAccount(): Promise<void> {
    if (this.snapshot.state === "deleting" || (this.snapshot.state === "browser_handoff" && this.snapshot.handoffPurpose === "delete")) {
      this.update({ ...this.snapshot, state: "signed_in_online", deleteStep: null, deleteError: null, handoffDeadline: null, handoffPurpose: "sign_in" });
    }
  }

  async finishSetup(): Promise<void> {
    if (this.snapshot.state === "signed_in_online") this.update({ ...this.snapshot, firstSignIn: false });
  }

  override async signOut(): Promise<void> { await this.signOutHere(); }

  private clearLocalAccountData(state: AccountSnapshot["state"]): void {
    this.update({
      ...emptySnapshot,
      state,
      serviceOpen: this.snapshot.serviceOpen,
      liveMatch: this.snapshot.liveMatch,
    });
  }

  private update(snapshot: AccountSnapshot): void {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener();
  }
}
