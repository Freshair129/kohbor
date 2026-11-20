import { Chip } from "../Readiness";
import type { ShellDestination, statusLineModel } from "./state";

type StatusLineModel = ReturnType<typeof statusLineModel>;

export function StatusLine({ model, label, hostLostMessage, onNavigate }: {
  model: StatusLineModel;
  label: string;
  hostLostMessage: string;
  onNavigate: (destination: ShellDestination) => void;
}) {
  return (
    <div className="shell-status-line" aria-label={label}>
      <ul className="shell-status-items" aria-label={label}>
        {model.items.map((item) => (
          <li key={item.id} className={`shell-status-item shell-status-item--${item.id}`} title={item.detail}>
            <Chip tone={item.tone} label={item.label} />
            {item.detail && <small className="shell-status-detail">{item.detail}</small>}
          </li>
        ))}
      </ul>
      {model.hostLost && <span className="shell-host-lost" role="alert">{hostLostMessage}</span>}
      {model.setupAction && <button type="button" className="shell-status-setup" onClick={() => onNavigate(model.setupAction!.destination)}>
        {model.setupAction.label}
      </button>}
    </div>
  );
}
