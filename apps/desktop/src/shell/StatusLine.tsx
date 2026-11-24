import { Chip } from "../Readiness";
import type { ShellDestination, statusLineModel } from "./state";
import "./header.css";

type StatusLineModel = ReturnType<typeof statusLineModel>;

export function StatusLine({ model, label, hostLostMessage, onNavigate }: {
  model: StatusLineModel;
  label: string;
  hostLostMessage: string;
  onNavigate: (destination: ShellDestination) => void;
}) {
  const tooltip = model.hostLost ? hostLostMessage : model.items
    .map((item) => item.detail ? `${item.label} · ${item.detail}` : item.label)
    .join(" · ");
  return (
    <div className="shell-status-line" aria-label={label} title={tooltip}>
      <ul className="shell-status-items" aria-label={label}>
        {model.items.map((item) => (
          <li key={item.id} className={`shell-status-item shell-status-item--${item.id}`} title={item.detail}>
            <Chip tone={item.tone} label={item.label} />
          </li>
        ))}
      </ul>
      {model.setupAction && <button type="button" className="shell-status-setup" onClick={() => onNavigate(model.setupAction!.destination)}>
        {model.setupAction.label}
      </button>}
    </div>
  );
}
