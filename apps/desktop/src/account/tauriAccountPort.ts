import {
  GuestAccountPort,
  guestAccountPort,
  type AccountPort,
  type AccountSnapshot,
  type AccountView,
} from "./accountView";

export type NativeAccountStatus = {
  state: "signed_out" | "browser_handoff" | "signing_in" | "signed_in" | "service_unavailable";
  displayName: string | null;
  avatarUrl: string | null;
  liveMatchBlocksHandoff: boolean;
};

export type AccountCommandInvoker = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

const initialSnapshot: AccountSnapshot = {
  ...guestAccountPort.getState(),
  serviceOpen: true,
};

/** Native account surface; tokens and Google email never cross this renderer boundary. */
export class TauriAccountPort extends GuestAccountPort implements AccountPort {
  readonly authOnly = true;
  private snapshot: AccountSnapshot = initialSnapshot;
  private avatarUrl: string | null = null;
  private readonly listeners = new Set<() => void>();
  private operationRevision = 0;
  private liveMatchOverride: boolean | null = null;
  private readonly invokeCommand: AccountCommandInvoker;

  constructor(invokeCommand: AccountCommandInvoker) {
    super();
    this.invokeCommand = invokeCommand;
  }

  override get = (): AccountView => {
    const state = this.snapshot.state;
    const signedIn = state === "signed_in_online";
    const canSignIn = (state === "signed_out" || state === "service_unavailable")
      && this.snapshot.serviceOpen && !this.snapshot.liveMatch;
    return {
      state: signedIn ? "online" : state === "signed_in_offline" ? "offline"
        : state === "session_revoked" ? "expired" : "guest",
      displayName: this.snapshot.displayName,
      avatar: this.avatarUrl,
      canSignIn,
      canSignOut: signedIn,
      friendsAvailable: false,
    };
  };

  override getState = (): AccountSnapshot => this.snapshot;

  override subscribe = (cb: () => void): (() => void) => {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  };

  async refreshStatus(): Promise<void> {
    const revision = this.operationRevision;
    try {
      const status = await this.invokeCommand<NativeAccountStatus>("account_status");
      if (revision === this.operationRevision) this.applyStatus(status);
    } catch {
      if (revision === this.operationRevision) this.markUnavailable();
    }
  }

  setLiveMatch(liveMatch: boolean): void {
    this.liveMatchOverride = liveMatch;
    this.update({ ...this.snapshot, liveMatch });
  }

  override async signIn(): Promise<void> {
    const state = this.snapshot.state;
    if (this.snapshot.liveMatch || (state !== "signed_out" && state !== "service_unavailable")) return;
    const revision = ++this.operationRevision;
    this.avatarUrl = null;
    this.update({
      ...this.snapshot,
      state: "browser_handoff",
      handoffDeadline: Date.now() + 5 * 60_000,
      handoffPurpose: "sign_in",
      notice: null,
      requestFailed: false,
    });
    try {
      const status = await this.invokeCommand<NativeAccountStatus>("account_sign_in");
      if (revision === this.operationRevision) this.applyStatus(status);
    } catch {
      if (revision === this.operationRevision) this.markUnavailable();
    }
  }

  override async cancelHandoff(): Promise<void> {
    if (this.snapshot.state !== "browser_handoff" && this.snapshot.state !== "signing_in") return;
    const revision = ++this.operationRevision;
    try {
      await this.invokeCommand<void>("account_cancel_sign_in");
      if (revision === this.operationRevision) {
        this.update({
          ...this.snapshot,
          state: "signed_out",
          handoffDeadline: null,
          notice: "cancelled",
          requestFailed: false,
        });
      }
    } catch {
      if (revision === this.operationRevision) this.markUnavailable();
      throw new Error("ACCOUNT_CANCEL_FAILED");
    }
  }

  override async signOutHere(): Promise<void> {
    const revision = ++this.operationRevision;
    try {
      await this.invokeCommand<void>("account_sign_out");
      if (revision === this.operationRevision) {
        this.avatarUrl = null;
        this.update({ ...initialSnapshot, liveMatch: this.snapshot.liveMatch });
      }
    } catch {
      if (revision === this.operationRevision) this.markUnavailable();
      throw new Error("ACCOUNT_SIGN_OUT_FAILED");
    }
  }

  override async retryConnection(): Promise<void> {
    await this.refreshStatus();
  }

  override async signOut(): Promise<void> {
    await this.signOutHere();
  }

  private applyStatus(status: NativeAccountStatus): void {
    const state: AccountSnapshot["state"] = status.state === "signed_in" ? "signed_in_online"
      : status.state === "signing_in" ? "signing_in" : status.state;
    this.avatarUrl = status.avatarUrl;
    this.update({
      ...initialSnapshot,
      state,
      serviceOpen: true,
      liveMatch: this.liveMatchOverride ?? status.liveMatchBlocksHandoff,
      displayName: status.displayName,
      handoffDeadline: state === "browser_handoff" ? Date.now() + 5 * 60_000 : null,
      requestFailed: state === "service_unavailable",
    });
  }

  private markUnavailable(): void {
    this.update({
      ...this.snapshot,
      state: "service_unavailable",
      handoffDeadline: null,
      requestFailed: true,
    });
  }

  private update(snapshot: AccountSnapshot): void {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener();
  }
}

export function accountPortForEnvironment(
  isTauri: boolean,
  invokeCommand?: AccountCommandInvoker,
): AccountPort {
  if (!isTauri) return guestAccountPort;
  if (!invokeCommand) throw new Error("TAURI_ACCOUNT_INVOKER_REQUIRED");
  return new TauriAccountPort(invokeCommand);
}
