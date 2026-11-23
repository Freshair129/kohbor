import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { reduceAccountMenu, type AccountMenuState } from "../account/menuModel";
import type { AccountPort, AccountView } from "../account/accountView";
import type { AppVersionView } from "./appVersionModel";
import { accountFlyoutItems, type AccountFlyoutAction } from "./accountFlyoutModel";

export type AccountFlyoutDestination = "account" | "settings";

export function AccountFlyout({ account, port, version, steamName, showSteamName, locale, labels, currentPage, navigate }: {
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
    presence: Record<AccountView["state"], string>;
  };
  currentPage: string;
  navigate: (page: AccountFlyoutDestination) => void;
}) {
  const items = useMemo(() => accountFlyoutItems(account), [account.canSignIn, account.canSignOut]);
  const [state, setState] = useState<AccountMenuState>({ open: false, index: 0 });
  const trigger = useRef<HTMLButtonElement | null>(null);
  const menu = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef<Partial<Record<AccountFlyoutAction, HTMLButtonElement | null>>>({});
  const stateText = labels.presence[account.state];

  const apply = (key: string) => {
    const next = reduceAccountMenu(state, key, items.length);
    setState(next.state);
    if (next.returnFocus) trigger.current?.focus();
  };

  useEffect(() => {
    if (state.open) itemRefs.current[items[state.index] ?? "account"]?.focus();
  }, [state.open, state.index, items]);

  useEffect(() => {
    if (!state.open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !menu.current?.contains(event.target) && !trigger.current?.contains(event.target)) {
        setState((current) => reduceAccountMenu(current, "outside", items.length).state);
      }
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [state.open, items.length]);

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement | HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End", "Escape", "Tab"].includes(event.key)) return;
    if (event.key !== "Tab") event.preventDefault();
    apply(event.key);
  };

  const activate = (action: AccountFlyoutAction) => {
    apply("select");
    if (action === "account" || action === "settings") navigate(action);
    if (action === "signIn") {
      navigate("account");
      void port.signIn().catch(() => {});
    }
    if (action === "signOut") void port.signOut().catch(() => {});
    trigger.current?.focus();
  };

  const labelFor = (action: AccountFlyoutAction) => ({
    account: labels.account,
    settings: labels.settings,
    signIn: labels.signIn,
    signOut: labels.signOut,
  })[action];

  return (
    <div className="shell-account">
      <button ref={trigger} type="button" className="shell-account-trigger" aria-label={`${labels.button}: ${stateText}`}
        aria-haspopup="menu" aria-expanded={state.open} aria-controls="shell-account-menu"
        aria-current={currentPage === "account" ? "page" : undefined}
        onClick={() => apply("toggle")} onKeyDown={(event) => {
          if (["ArrowDown", "ArrowUp", "Escape"].includes(event.key)) handleKeyDown(event);
        }}>
        <span className="shell-account-avatar" aria-hidden="true">{account.displayName?.slice(0, 1) || "?"}</span>
        <span className="shell-account-presence" aria-hidden="true" />
      </button>
      {state.open && <div ref={menu} id="shell-account-menu" className="shell-account-menu" role="menu" aria-label={labels.menu}
        onKeyDown={handleKeyDown}>
        <div className="shell-account-summary" role="presentation">
          <span className="shell-account-avatar shell-account-avatar-large" aria-hidden="true">{account.displayName?.slice(0, 1) || "?"}</span>
          <div className="shell-account-summary-text">
            <strong>{account.displayName ?? stateText}</strong>
            <span role="status">{stateText}</span>
            {showSteamName && account.state !== "guest" && <small>Steam: {steamName ?? "—"}</small>}
          </div>
        </div>
        {items.map((action, index) => <button key={action} ref={(element) => { itemRefs.current[action] = element; }} type="button"
          role="menuitem" tabIndex={-1} aria-current={action === "account" && currentPage === "account" ? "page" : undefined}
          onFocus={() => setState((current) => current.index === index ? current : { ...current, index })}
          onClick={() => activate(action)}>
          <span>{labelFor(action)}</span><span aria-hidden="true">›</span>
        </button>)}
        <div className="shell-account-version" role="presentation">
          <span>{labels.version}: {version.compact}</span>
          {version.demoBadge && <b className="shell-version-demo">{version.demoBadge}</b>}
          <small>commit {version.commit}</small>
        </div>
      </div>}
    </div>
  );
}
