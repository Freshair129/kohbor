import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { dialogKeyAction, isBackdropClick, wrapFocusIndex } from "./dialogModel";
import "./dialog.css";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Modal dialog with a visible close button, Esc, click-outside, a focus trap and focus
 * restoration to the element that opened it. It lives in the normal DOM tree (fixed layer),
 * so it can never trap the user without an exit.
 */
export function Dialog({ title, closeLabel, onClose, children, className }: {
  title: string;
  closeLabel: string;
  onClose: () => void;
  children: ReactNode;
  className?: string;
}) {
  const titleId = useId();
  const panel = useRef<HTMLDivElement | null>(null);
  const backdrop = useRef<HTMLDivElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.querySelector<HTMLElement>(".dialog-close")?.focus();
    const onKey = (event: KeyboardEvent) => {
      const action = dialogKeyAction(event.key, event.shiftKey);
      if (action === null) return;
      if (action === "close") {
        event.preventDefault();
        event.stopPropagation();
        closeRef.current();
        return;
      }
      const items = Array.from(panel.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
      const next = wrapFocusIndex(action, items.indexOf(document.activeElement as HTMLElement), items.length);
      if (next !== null) {
        event.preventDefault();
        items[next]?.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  return (
    <div
      className="dialog-backdrop"
      ref={backdrop}
      onMouseDown={(event) => { if (isBackdropClick(event.target, backdrop.current)) closeRef.current(); }}
    >
      <div className={`dialog-panel${className ? ` ${className}` : ""}`} role="dialog" aria-modal="true" aria-labelledby={titleId} ref={panel}>
        <header className="dialog-head">
          <h2 id={titleId}>{title}</h2>
          <button type="button" className="dialog-close" onClick={() => closeRef.current()} aria-label={closeLabel}>✕ {closeLabel}</button>
        </header>
        <div className="dialog-body">{children}</div>
      </div>
    </div>
  );
}

/** A trigger button that opens a {@link Dialog}; focus returns to the button on close. */
export function DialogButton({ label, title, closeLabel, className, children }: {
  label: string;
  title?: string;
  closeLabel: string;
  className?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" className="dialog-trigger" aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}>{label}</button>
      {open && <Dialog title={title ?? label} closeLabel={closeLabel} className={className} onClose={() => setOpen(false)}>{children}</Dialog>}
    </>
  );
}
