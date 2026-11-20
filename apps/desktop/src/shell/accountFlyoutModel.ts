import type { AccountView } from "../account/accountView";

export type AccountFlyoutAction = "account" | "settings" | "signIn" | "signOut";

/** Only account actions backed by the current account port are offered. */
export function accountFlyoutItems(account: Pick<AccountView, "canSignIn" | "canSignOut">): readonly AccountFlyoutAction[] {
  const items: AccountFlyoutAction[] = ["account", "settings"];
  if (account.canSignIn) items.push("signIn");
  if (account.canSignOut) items.push("signOut");
  return items;
}
