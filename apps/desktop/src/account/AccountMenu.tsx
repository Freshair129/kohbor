import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import type { AppVersionView } from "../shell/appVersionModel";
import { accountSnapshotFromLegacy, type AccountPort, type AccountView } from "./accountView";
import { accountControlStatus } from "./accountViewModel";
import { reduceAccountMenu } from "./menuModel";
import "./account.css";

export type AccountDestination = "account" | "friends" | "settings";

/** Right-rail control only reports the account state and opens its destination. */
export function AccountMenu({ account, port, version, locale, navigate }: {
  account: AccountView;
  port: AccountPort;
  version: AppVersionView;
  locale: "th" | "en";
  navigate: (page: AccountDestination) => void;
}) {
  const th = locale === "th";
  const snapshot = port.getState() ?? accountSnapshotFromLegacy(account);
  const [state, setState] = useState({ open: false, index: 0 });
  const [top, setTop] = useState(64);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const menu = useRef<HTMLDivElement | null>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  const destinations: { page: AccountDestination; label: string; caption?: string }[] = [
    { page: "account", label: th ? "บัญชีของฉัน" : "My account" },
    { page: "friends", label: th ? "เพื่อน" : "Friends", caption: th ? "เร็วๆ นี้" : "Coming soon" },
    { page: "settings", label: th ? "ตั้งค่า" : "Settings" },
  ];

  const apply = (key: string) => {
    const next = reduceAccountMenu(state, key, destinations.length);
    setState(next.state);
    if (next.returnFocus) trigger.current?.focus();
  };

  useLayoutEffect(() => {
    if (!state.open) return;
    setTop((trigger.current?.getBoundingClientRect().bottom ?? 56) + 8);
    items.current[state.index]?.focus();
  }, [state.open, state.index]);

  useEffect(() => {
    if (!state.open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !menu.current?.contains(event.target) && !trigger.current?.contains(event.target)) {
        setState((current) => reduceAccountMenu(current, "outside", destinations.length).state);
      }
    };
    const reposition = () => setTop((trigger.current?.getBoundingClientRect().bottom ?? 56) + 8);
    document.addEventListener("pointerdown", outside);
    window.addEventListener("resize", reposition);
    return () => { document.removeEventListener("pointerdown", outside); window.removeEventListener("resize", reposition); };
  }, [state.open, destinations.length]);

  const keyboard = (event: KeyboardEvent) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End", "Escape", "Tab"].includes(event.key)) return;
    if (event.key !== "Tab") event.preventDefault();
    apply(event.key);
  };

  const stateLabel = accountControlStatus(snapshot, locale);
  const displayName = snapshot.displayName;
  return <>
    <button ref={trigger} type="button" className="account-trigger" lang={locale} aria-haspopup="menu" aria-expanded={state.open}
      aria-controls="account-menu" onClick={() => apply("toggle")} onKeyDown={(event) => {
        if (["ArrowDown", "ArrowUp", "Escape"].includes(event.key)) keyboard(event);
      }}>
      <span className="account-avatar" aria-hidden="true">?</span>
      <span>{displayName ?? stateLabel}</span><span aria-hidden="true">⌄</span>
    </button>
    {state.open && createPortal(
      <div ref={menu} id="account-menu" className="account-menu" lang={locale} role="menu" aria-label={th ? "บัญชี" : "Account"}
        style={{ top, maxHeight: `calc(100dvh - ${top + 12}px)` }} onKeyDown={keyboard}>
        <div className="account-menu-profile" role="presentation">
          <strong>{stateLabel}</strong>
          {displayName !== null && <small>{displayName}</small>}
        </div>
        {destinations.map((item, index) => <button key={item.page} ref={(element) => { items.current[index] = element; }}
          type="button" role="menuitem" tabIndex={-1} onFocus={() => setState((current) => current.index === index ? current : { ...current, index })}
          onClick={() => { apply("select"); navigate(item.page); }}>
          <span>{item.label}</span>{item.caption && <small>{item.caption}</small>}<span aria-hidden="true">›</span>
        </button>)}
        <div className="account-menu-version" role="presentation">{th ? "เวอร์ชัน" : "Version"}: {version.compact}
          {version.demoBadge && <b className="shell-version-demo">{version.demoBadge}</b>}<small>commit {version.commit}</small>
        </div>
      </div>, document.body)}
  </>;
}
