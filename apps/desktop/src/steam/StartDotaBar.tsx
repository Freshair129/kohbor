import { deriveLaunchView, deriveSteamChips, launchResultText, steamPersonaName, type SteamStatus } from "./model";
import type { useDotaLauncher } from "./hooks";
import "./steam.css";
export function StartDotaBar({ status, control, showSteamName, locale }: {
  status: SteamStatus | null; control: ReturnType<typeof useDotaLauncher>; showSteamName: boolean; locale: "th" | "en";
}) {
  const chips = deriveSteamChips(status, locale);
  const view = deriveLaunchView(status, control.state.pending || control.state.awaitingStatus, locale);
  const persona = steamPersonaName(status, showSteamName);
  const result = control.state.attempted && !control.state.pending ? launchResultText(control.state.result, locale) : null;
  const title = [view.reason, showSteamName ? `Steam: ${persona ?? "—"}` : null].filter(Boolean).join(" · ") || undefined;
  return <div className="steam-start-bar">
    <div className="steam-service-chips" aria-label={locale === "th" ? "สถานะเกม" : "Game status"}>
      <span>Steam: <b>{chips.steam}</b></span><span>Dota 2: <b>{chips.dota}</b></span><span>GSI: <b>{chips.gsi}</b></span>
    </div>
    <span className="steam-launch-message" role="status" title={result ?? undefined}>{result}</span>
    <button type="button" disabled={!view.enabled} title={title} aria-describedby="start-dota-reason"
      onClick={() => void control.controller.launch(status)}>{locale === "th" ? "เริ่ม Dota 2" : "Start Dota 2"}</button>
    <span id="start-dota-reason" className="steam-launch-reason">{view.reason}</span>
  </div>;
}
