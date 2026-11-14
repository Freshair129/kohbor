import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import type { AppVersionView } from "../shell/appVersionModel";
import { accountStateText, type AccountPort, type AccountView } from "./accountView";
import { reduceAccountMenu } from "./menuModel";
import "./account.css";

export type AccountDestination = "account" | "friends" | "settings";
export function AccountMenu({ account, port, version, steamName, showSteamName, locale, navigate }: {
  account: AccountView; port: AccountPort; version: AppVersionView; steamName: string | null;
  showSteamName: boolean; locale: "th" | "en"; navigate: (page: AccountDestination) => void;
}) {
  const th = locale === "th";
  const [state, setState] = useState({ open: false, index: 0 });
  const [top, setTop] = useState(64);
  const trigger = useRef<HTMLButtonElement | null>(null);
  const menu = useRef<HTMLDivElement | null>(null);
  const items = useRef<(HTMLButtonElement | null)[]>([]);
  const count = account.canSignOut ? 4 : 3;
  const apply = (key: string) => {
    const next = reduceAccountMenu(state, key, count);
    setState(next.state);
    if (next.returnFocus) trigger.current?.focus();
  };
  useLayoutEffect(() => {
    if (state.open) { setTop((trigger.current?.getBoundingClientRect().bottom ?? 56) + 8); items.current[state.index]?.focus(); }
  }, [state.open, state.index]);
  useEffect(() => {
    if (!state.open) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !menu.current?.contains(event.target) && !trigger.current?.contains(event.target)) {
        setState(current => reduceAccountMenu(current, "outside", count).state);
      }
    };
    const reposition = () => setTop((trigger.current?.getBoundingClientRect().bottom ?? 56) + 8);
    document.addEventListener("pointerdown", outside);
    window.addEventListener("resize", reposition);
    return () => { document.removeEventListener("pointerdown", outside); window.removeEventListener("resize", reposition); };
  }, [state.open, count]);
  const keyboard = (event: KeyboardEvent) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End", "Escape", "Tab"].includes(event.key)) return;
    if (event.key !== "Tab") event.preventDefault();
    apply(event.key);
  };
  const destinations: { page: AccountDestination; label: string; caption?: string }[] = [
    { page: "account", label: th ? "บัญชีของฉัน" : "My account" },
    { page: "friends", label: th ? "เพื่อน" : "Friends", caption: th ? "เร็วๆ นี้" : "Coming soon" },
    { page: "settings", label: th ? "ตั้งค่า" : "Settings" },
  ];
  const accountLabel = accountStateText(account, locale);
  return <>
    <button ref={trigger} type="button" className="account-trigger" aria-haspopup="menu" aria-expanded={state.open}
      aria-controls="account-menu" onClick={() => apply("toggle")} onKeyDown={event => {
        if (["ArrowDown", "ArrowUp", "Escape"].includes(event.key)) keyboard(event);
      }}>
      <span className="account-avatar" aria-hidden="true">?</span><span>{account.displayName ?? accountLabel}</span><span aria-hidden="true">⌄</span>
    </button>
    {state.open && createPortal(
      <div ref={menu} id="account-menu" className="account-menu" role="menu" aria-label={th ? "บัญชี" : "Account"}
        style={{ top, maxHeight: `calc(100dvh - ${top + 12}px)` }} onKeyDown={keyboard}>
        <div className="account-menu-profile" role="presentation"><strong>{accountLabel}</strong>
          {showSteamName && <small>Steam: {steamName ?? "—"}</small>}
          <button type="button" role="menuitem" disabled={!account.canSignIn} onClick={() => void port.signIn().catch(() => {})}>
            {th ? "เข้าสู่ระบบด้วย Google" : "Sign in with Google"}
          </button><small>{th ? "บริการยังไม่เปิด" : "Service not available yet"}</small>
        </div>
        {destinations.map((item, index) => <button key={item.page} ref={element => { items.current[index] = element; }}
          type="button" role="menuitem" tabIndex={-1} onFocus={() => setState(current => current.index === index ? current : { ...current, index })}
          onClick={() => { apply("select"); navigate(item.page); }}>
          <span>{item.label}</span>{item.caption && <small>{item.caption}</small>}<span aria-hidden="true">›</span>
        </button>)}
        <div className="account-menu-version" role="presentation">{th ? "เวอร์ชัน" : "Version"}: {version.compact}
          {version.demoBadge && <b className="shell-version-demo">{version.demoBadge}</b>}<small>commit {version.commit}</small>
        </div>
        <button ref={element => { items.current[3] = element; }} type="button" role="menuitem" tabIndex={-1} disabled={!account.canSignOut}
          onClick={() => { void port.signOut().catch(() => {}); apply("select"); }}>{th ? "ออกจากระบบ" : "Sign out"}</button>
      </div>, document.body)}
  </>;
}
