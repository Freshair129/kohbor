export type AccountUiState =
  | "signed_out"
  | "browser_handoff"
  | "signing_in"
  | "needs_display_name"
  | "signed_in_online"
  | "signed_in_offline"
  | "service_unavailable"
  | "session_revoked"
  | "paused_new_device"
  | "deleting";

export type FriendCodeSnapshot = {
  value: string | null;
  freshness: "live" | "cached" | "unknown";
  isExample: boolean;
};

export type AccountSession = {
  thisDevice: boolean;
  signedInOn: string | null;
};

export type AccountSnapshot = {
  state: AccountUiState;
  serviceOpen: boolean;
  liveMatch: boolean;
  invitedOnly: boolean;
  displayName: string | null;
  friendCode: FriendCodeSnapshot;
  sessions: readonly AccountSession[] | null;
  handoffDeadline: number | null;
  handoffPurpose: "sign_in" | "delete";
  firstSignIn: boolean;
  dialog: "replace_code" | "sign_out_everywhere" | "export" | null;
  nameError: "empty" | "long" | "refused" | "not_saved" | null;
  notice: "cancelled" | "deleted" | "provider_declined" | null;
  rateLimitDeadline: number | null;
  deleteStep: "review" | "reauth" | "confirm" | null;
  deleteError: "mismatch" | "window" | "failed" | null;
  freshSignInDeadline: number | null;
  codeReplacementsRemaining: number | null;
  requestFailed: boolean;
};

export type AccountActionResult =
  | { ok: true }
  | { ok: false; reason: "service_closed" | "live_match" | "invalid_state" | "limit_reached" | "failed" };

/** Legacy shell summary. Detailed destination state remains in AccountSnapshot. */
export type AccountView = {
  state: "guest" | "online" | "offline" | "expired";
  displayName: string | null;
  avatar: string | null;
  canSignIn: boolean;
  canSignOut: boolean;
  friendsAvailable: boolean;
};

export interface AccountPort {
  /** Supabase Auth alpha exposes sign-in and sign-out only; the remaining account actions are not enabled. */
  readonly authOnly?: boolean;
  get(): AccountView;
  getState(): AccountSnapshot;
  subscribe(cb: () => void): () => void;
  refreshStatus?(): Promise<void>;
  setLiveMatch?(liveMatch: boolean): void;
  signIn(): Promise<void>;
  cancelHandoff(): Promise<void>;
  saveDisplayName(name: string): Promise<AccountActionResult>;
  replaceFriendCode(): Promise<AccountActionResult>;
  listSessions(): Promise<readonly AccountSession[] | null>;
  retryConnection(): Promise<void>;
  signOutHere(): Promise<void>;
  signOutEverywhere(): Promise<void>;
  exportAccount(): Promise<AccountActionResult>;
  beginDelete(): Promise<void>;
  deleteWithFreshSignIn(): Promise<void>;
  confirmDelete(): Promise<AccountActionResult>;
  keepAccount(): Promise<void>;
  finishSetup(): Promise<void>;
  /** Compatibility alias retained for the current shell. */
  signOut(): Promise<void>;
}

const guestSnapshot: AccountSnapshot = Object.freeze({
  state: "signed_out",
  serviceOpen: false,
  liveMatch: false,
  invitedOnly: false,
  displayName: null,
  friendCode: Object.freeze({ value: null, freshness: "unknown", isExample: false }),
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
  codeReplacementsRemaining: null,
  requestFailed: false,
});

const guestView: AccountView = Object.freeze({
  state: "guest",
  displayName: null,
  avatar: null,
  canSignIn: false,
  canSignOut: false,
  friendsAvailable: false,
});

/** Reserved default boundary. The guest port has no network, storage or authentication path. */
export class GuestAccountPort implements AccountPort {
  readonly authOnly: boolean = false;
  get = (): AccountView => guestView;
  getState = (): AccountSnapshot => guestSnapshot;
  subscribe = (_cb: () => void): (() => void) => () => {};
  async signIn(): Promise<void> {}
  async cancelHandoff(): Promise<void> {}
  async saveDisplayName(_name: string): Promise<AccountActionResult> { return { ok: false, reason: "service_closed" }; }
  async replaceFriendCode(): Promise<AccountActionResult> { return { ok: false, reason: "service_closed" }; }
  async listSessions(): Promise<readonly AccountSession[] | null> { return null; }
  async retryConnection(): Promise<void> {}
  async signOutHere(): Promise<void> {}
  async signOutEverywhere(): Promise<void> {}
  async exportAccount(): Promise<AccountActionResult> { return { ok: false, reason: "service_closed" }; }
  async beginDelete(): Promise<void> {}
  async deleteWithFreshSignIn(): Promise<void> {}
  async confirmDelete(): Promise<AccountActionResult> { return { ok: false, reason: "service_closed" }; }
  async keepAccount(): Promise<void> {}
  async finishSetup(): Promise<void> {}
  async signOut(): Promise<void> { await this.signOutHere(); }
}

export const guestAccountPort: AccountPort = new GuestAccountPort();

export function accountStateText(account: AccountView, locale: "th" | "en"): string {
  const labels = locale === "th"
    ? { guest: "ยังไม่ได้เข้าสู่ระบบ", online: "ออนไลน์", offline: "ออฟไลน์", expired: "เซสชันหมดอายุ" }
    : { guest: "Not signed in", online: "Online", offline: "Offline", expired: "Session expired" };
  return labels[account.state];
}

export function legacyStateFor(snapshot: AccountSnapshot): AccountView["state"] {
  switch (snapshot.state) {
    case "signed_in_online": return "online";
    case "signed_in_offline": return "offline";
    case "session_revoked": return "expired";
    default: return "guest";
  }
}

export function accountSnapshotFromLegacy(view: AccountView): AccountSnapshot {
  const state: AccountSnapshot["state"] = view.state === "online" ? "signed_in_online"
    : view.state === "offline" ? "signed_in_offline"
      : view.state === "expired" ? "session_revoked" : "signed_out";
  return {
    ...guestSnapshot,
    state,
    serviceOpen: view.canSignIn,
    displayName: view.displayName,
  };
}
