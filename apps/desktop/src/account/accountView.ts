export type AccountView = {
  state: "guest" | "online" | "offline" | "expired";
  displayName: string | null;
  avatar: null;
  canSignIn: boolean;
  canSignOut: boolean;
  friendsAvailable: boolean;
};
export interface AccountPort {
  get(): AccountView;
  subscribe(cb: () => void): () => void;
  signIn(): Promise<void>;
  signOut(): Promise<void>;
}
const guest: AccountView = Object.freeze({ state: "guest", displayName: null, avatar: null,
  canSignIn: false, canSignOut: false, friendsAvailable: false });

/** Reserved boundary for kohbor-account. No authentication, identifiers, storage or network. */
export class GuestAccountPort implements AccountPort {
  get = (): AccountView => guest;
  subscribe = (_cb: () => void): (() => void) => () => {};
  async signIn(): Promise<void> { /* Service unopened; the guest view cannot sign in. */ }
  async signOut(): Promise<void> { /* Guest is already signed out. */ }
}
export const guestAccountPort: AccountPort = new GuestAccountPort();

export function accountStateText(account: AccountView, locale: "th" | "en"): string {
  const labels = locale === "th" ? { guest: "ยังไม่ได้เข้าสู่ระบบ", online: "ออนไลน์", offline: "ออฟไลน์", expired: "เซสชันหมดอายุ" }
    : { guest: "Not signed in", online: "Online", offline: "Offline", expired: "Session expired" };
  return labels[account.state];
}
