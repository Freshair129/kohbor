import type { AccountPort, AccountView } from "../account/accountView";
import type { AppVersionView } from "./appVersionModel";
import { AccountFlyout, type AccountFlyoutDestination } from "./AccountFlyout";
import { RailRibbon } from "./RailRibbon";

export function RailRight({ account, port, version, steamName, showSteamName, locale, labels, currentPage, navigate }: {
  account: AccountView;
  port: AccountPort;
  version: AppVersionView;
  steamName: string | null;
  showSteamName: boolean;
  locale: "th" | "en";
  labels: {
    button: string;
    menu: string;
    account: string;
    settings: string;
    signIn: string;
    signOut: string;
    version: string;
    friends: string;
    friendsNotYet: string;
    presence: Record<AccountView["state"], string>;
  };
  currentPage: string;
  navigate: (page: AccountFlyoutDestination) => void;
}) {
  return (
    <aside className="shell-rail shell-rail-right">
      <AccountFlyout account={account} port={port} version={version} steamName={steamName} showSteamName={showSteamName}
        locale={locale} labels={labels} currentPage={currentPage} navigate={navigate} />
      <section className="shell-rail-friends" aria-label={labels.friends} data-slot="friends-reserved" title={labels.friendsNotYet}>
        <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
          <circle cx="9" cy="8" r="3.2" /><path d="M3.5 19a5.5 5.5 0 0 1 11 0" /><circle cx="17" cy="9" r="2.4" /><path d="M15.5 14.2A4.5 4.5 0 0 1 21 18.5" />
        </svg>
        <span className="shell-rail-friends-label">{labels.friends}</span>
        <span className="shell-rail-friends-note">{labels.friendsNotYet}</span>
      </section>
      <RailRibbon side="right" />
    </aside>
  );
}
