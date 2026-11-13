import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { EventBanner } from "./EventBanner";
import { MediaProbeContext } from "./MediaProbeContext";
import { usePlayback } from "./runtime";
import "./styles.css";

const root = document.getElementById("root");
const params = new URLSearchParams(window.location.search);
const overlayView = params.get("view") === "overlay";
document.documentElement.classList.toggle("overlay-body", overlayView);
document.body.classList.toggle("overlay-body", overlayView);

function OverlayView() {
  const playback = usePlayback();
  return <EventBanner playback={playback} />;
}

async function start() {
  const mockHarness = import.meta.env.DEV && params.get("mock") === "1"
    ? await import("./mock/harness")
    : null;
  mockHarness?.installMockHarness();

  if (!root) return;
  const MockScenarioSwitcher = mockHarness?.MockScenarioSwitcher;
  createRoot(root).render(
    <StrictMode>
      {overlayView ? (
        <OverlayView />
      ) : (
        <>
          <MediaProbeContext />
          <App devScenarioSwitcher={MockScenarioSwitcher ? <MockScenarioSwitcher /> : undefined} />
        </>
      )}
    </StrictMode>,
  );
}

void start();
