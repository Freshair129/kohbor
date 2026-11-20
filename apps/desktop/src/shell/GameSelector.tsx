import { useEffect, useRef, useState } from "react";
import { gameMenuItems, gameStatusLabel, unsupportedGameExplanation, type DotaConnection } from "./game-selector";

export function GameSelector({ connection, locale }: { connection: DotaConnection; locale: "th" | "en" }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const items = gameMenuItems(connection);

  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>("[role=radio]")?.focus();
    const outside = (event: PointerEvent) => {
      if (!menu.current?.contains(event.target as Node) && !trigger.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);

  const close = () => {
    setOpen(false);
    trigger.current?.focus();
  };

  const handleKey = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const buttons = Array.from(menu.current?.querySelectorAll<HTMLButtonElement>("button[data-game]") ?? []);
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") { event.preventDefault(); close(); }
    else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && buttons.length) {
      event.preventDefault();
      buttons[(index + (event.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
  };

  return <div className="shell-game-selector">
    <button ref={trigger} className="shell-game-trigger" type="button" aria-label={locale === "th" ? "เลือกเกม: Dota 2" : "Select game: Dota 2"}
      aria-haspopup="dialog" aria-expanded={open} onClick={() => { setNote(null); setOpen(value => !value); }}>
      <span className="shell-game-tile" aria-hidden="true">D2</span>
      <span>Dota 2 <span aria-hidden="true">⌄</span></span>
    </button>
    {open && <div ref={menu} className="shell-game-menu glass--raised glass-pop" role="dialog"
      aria-label={locale === "th" ? "เกม" : "Games"} onKeyDown={handleKey}>
      <h2>{locale === "th" ? "เกม" : "Games"}</h2>
      <h3>{locale === "th" ? "เกมที่เชื่อมต่อ" : "Connected game"}</h3>
      <button type="button" role="radio" aria-checked="true" data-game="dota2" onClick={close}>
        <span className="shell-game-tile" aria-hidden="true">D2</span>
        <span className="shell-game-name">Dota 2</span>
        <span className="shell-game-status">{gameStatusLabel(items[0]!.status, locale)}</span>
      </button>
      <h3>{locale === "th" ? "เกมอื่น — ยังไม่รองรับ" : "Other games — not supported"}</h3>
      <ul>
        {items.slice(1).map(item => <li key={item.id}>
          <button type="button" data-game={item.id} aria-disabled="true" onClick={() => setNote(unsupportedGameExplanation(item.name, locale))}>
            <span className="shell-game-tile shell-game-tile--unsupported" aria-hidden="true">{item.initials}</span>
            <span className="shell-game-name">{item.name}</span>
            <span className="shell-game-status">{gameStatusLabel(item.status, locale)}</span>
          </button>
        </li>)}
      </ul>
      {note && <p className="shell-game-note" role="status">{note}</p>}
      <p className="shell-game-trademark">{locale === "th" ? "ชื่อเกมใช้เพื่อระบุเกมเท่านั้น" : "Game names are used for identification only."}</p>
    </div>}
  </div>;
}
