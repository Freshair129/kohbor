import { useRef, type KeyboardEvent } from "react";
import brandMark from "../assets/brand/kohbor-rail-mark.svg";
import {
  navigationItemState,
  nextShellDestination,
  SHELL_NAV_ITEMS,
  type ShellDestination,
} from "./state";
import { RailRibbon } from "./RailRibbon";
import { GameSelector } from "./GameSelector";
import type { DotaConnection } from "./game-selector";

function DestinationIcon({ destination }: { destination: ShellDestination }) {
  const common = { width: 20, height: 20, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeWidth: 1.8, strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true as const, focusable: false as const };
  switch (destination) {
    case "overview": return <svg {...common}><rect x="3" y="3" width="7" height="7" rx="1.5" /><rect x="14" y="3" width="7" height="7" rx="1.5" /><rect x="3" y="14" width="7" height="7" rx="1.5" /><rect x="14" y="14" width="7" height="7" rx="1.5" /></svg>;
    case "live": return <svg {...common}><path d="M3 12h4l3-7 4 14 3-7h4" /><circle cx="3" cy="12" r="1" /></svg>;
    case "packs": return <svg {...common}><path d="M4 14v-2a8 8 0 0 1 16 0v2" /><path d="M4 13h3v7H6a2 2 0 0 1-2-2zM20 13h-3v7h1a2 2 0 0 0 2-2z" /></svg>;
    case "insights": return <svg {...common}><path d="M4 20V11M10 20V5M16 20v-8M22 20H2" /></svg>;
    case "store": return <svg {...common}><path d="M4 9h16l-1 12H5z" /><path d="M9 9a3 3 0 0 1 6 0" /></svg>;
    case "status": return <svg {...common}><path d="M3 12h4l3-7 4 14 3-7h4" /></svg>;
    case "logs": return <svg {...common}><path d="M8 6h12M8 12h12M8 18h12" /><path d="M4 6h.01M4 12h.01M4 18h.01" /></svg>;
    case "settings": return <svg {...common}><path d="M4 7h10M18 7h2M4 17h2M10 17h10" /><circle cx="16" cy="7" r="2" /><circle cx="8" cy="17" r="2" /></svg>;
  }
}

export function RailLeft({
  current,
  labels,
  navigationLabel,
  gameConnection,
  locale,
  railState,
  onToggleRail,
  onNavigate,
}: {
  current: ShellDestination | "account";
  labels: Record<ShellDestination, string>;
  navigationLabel: string;
  gameConnection: DotaConnection;
  locale: "th" | "en";
  railState: "collapsed" | "expanded";
  onToggleRail: () => void;
  onNavigate: (destination: ShellDestination) => void;
}) {
  const buttons = useRef<Partial<Record<ShellDestination, HTMLButtonElement | null>>>({});
  const expanded = railState === "expanded";
  const toggleLabel = locale === "th"
    ? expanded ? "ย่อแถบเมนู (Ctrl+B)" : "ขยายแถบเมนู (Ctrl+B)"
    : expanded ? "Collapse navigation rail (Ctrl+B)" : "Expand navigation rail (Ctrl+B)";

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, destination: ShellDestination) => {
    const next = nextShellDestination(destination, event.key);
    if (next === null) return;
    event.preventDefault();
    onNavigate(next);
    buttons.current[next]?.focus();
  };

  return (
    <aside id="rail" className="shell-rail shell-rail-left" aria-label={navigationLabel}>
      <div className="shell-rail-wordmark" role="img" aria-label="Kohbor / เกาะเบาะ">
        <img className="shell-rail-mark" src={brandMark} alt="" />
        <span>Kohbor</span><span lang="th">เกาะเบาะ</span>
      </div>
      <div className="shell-rail-section shell-rail-game-section" aria-hidden="true">{locale === "th" ? "เกม" : "Game"}</div>
      <GameSelector connection={gameConnection} locale={locale} />
      <nav className="shell-rail-nav" aria-label={navigationLabel}>
        <div className="shell-rail-group">
          <div className="shell-rail-section" aria-hidden="true">{locale === "th" ? "เมนูหลัก" : "Main"}</div>
          {SHELL_NAV_ITEMS.filter((item) => item.group === "primary").map(({ id }) => {
            const state = navigationItemState(id, current);
            return <button key={id} ref={(element) => { buttons.current[id] = element; }} type="button"
              className="shell-rail-link" data-destination={id} aria-current={state.ariaCurrent} tabIndex={state.tabIndex}
              onClick={() => onNavigate(id)} onKeyDown={(event) => handleKeyDown(event, id)}>
              <DestinationIcon destination={id} /><span>{labels[id]}</span>
              {id === "store" && <span className="shell-rail-store-chip">{locale === "th" ? "ยังไม่เปิด" : "Not open yet"}</span>}
            </button>;
          })}
        </div>
        <div className="shell-rail-group shell-rail-group-secondary">
          <div className="shell-rail-section" aria-hidden="true">{locale === "th" ? "ระบบ" : "System"}</div>
          {SHELL_NAV_ITEMS.filter((item) => item.group === "secondary").map(({ id }) => {
            const state = navigationItemState(id, current);
            return <button key={id} ref={(element) => { buttons.current[id] = element; }} type="button"
              className="shell-rail-link" data-destination={id} aria-current={state.ariaCurrent} tabIndex={state.tabIndex}
              onClick={() => onNavigate(id)} onKeyDown={(event) => handleKeyDown(event, id)}>
              <DestinationIcon destination={id} /><span>{labels[id]}</span>
            </button>;
          })}
        </div>
      </nav>
      <button type="button" className="shell-rail-toggle" aria-expanded={expanded} aria-controls="rail"
        aria-label={toggleLabel} title={toggleLabel} onClick={onToggleRail}>
        <span className="shell-rail-toggle-chevron" aria-hidden="true">{expanded ? "«" : "»"}</span>
        <span className="shell-rail-toggle-label">{locale === "th" ? "ย่อแถบเมนู" : "Collapse menu"}</span>
        <kbd aria-hidden="true">Ctrl+B</kbd>
      </button>
      <RailRibbon side="left" />
    </aside>
  );
}
