import { useEffect, useRef } from "react";
import { toWebviewUrl } from "./mediaUrl";
import {
  buildProbeReport,
  createProbeQueue,
  durationSecondsToMilliseconds,
  MEDIA_PROBE_TIMEOUT_MS,
  parsePendingProbeRequests,
  parseProbeRequest,
  withProbeTimeout,
  type MediaProbeItem,
  type MediaProbePendingResponse,
  type MediaProbeRequest,
  type ProbeClock,
} from "./probe";
import { mediaProbePending, reportMediaProbe, subscribeMediaProbeRequest } from "./runtime";

interface QueuedProbe {
  request: MediaProbeRequest;
  item: MediaProbeItem;
}

interface DecodeResult {
  ok: boolean;
  duration_ms: number;
}

let pendingFetch: Promise<MediaProbePendingResponse> | null = null;
let mountedContexts = 0;

function pendingOnceForMountedContext(): Promise<MediaProbePendingResponse> {
  pendingFetch ??= mediaProbePending();
  return pendingFetch;
}

function releasePendingContext() {
  mountedContexts -= 1;
  queueMicrotask(() => {
    if (mountedContexts === 0) pendingFetch = null;
  });
}

function webviewPlatform(): "windows" | "other" {
  return typeof navigator !== "undefined" && navigator.userAgent.includes("Windows") ? "windows" : "other";
}

function browserClock(): ProbeClock {
  return {
    setTimeout: (callback, delay_ms) => window.setTimeout(callback, delay_ms),
    clearTimeout: (handle) => window.clearTimeout(handle as number),
  };
}

function beginDecode(videoHost: HTMLElement, url: string): { promise: Promise<DecodeResult>; cancel: () => void } {
  const video = document.createElement("video");
  // The dev mock short-circuits its unavailable protocol to avoid a noisy localhost network error.
  const mockUnavailable = typeof window !== "undefined"
    && (window as Window & { __KOHBOR_MOCK_MEDIA_UNAVAILABLE__?: boolean }).__KOHBOR_MOCK_MEDIA_UNAVAILABLE__ === true;
  video.muted = true;
  video.defaultMuted = true;
  video.controls = false;
  video.autoplay = false;
  video.playsInline = true;
  video.preload = mockUnavailable ? "none" : "metadata";
  video.setAttribute("aria-hidden", "true");
  video.style.width = "1px";
  video.style.height = "1px";

  let frameCallbackId: number | undefined;
  let settled = false;
  let metadataDuration: number | null = null;
  let firstFrameDecoded = false;
  let resolveDecode: (result: DecodeResult) => void = () => {};

  const cleanup = () => {
    video.removeEventListener("loadedmetadata", onLoadedMetadata);
    video.removeEventListener("loadeddata", onLoadedData);
    video.removeEventListener("error", onError);
    video.removeEventListener("abort", onError);
    if (frameCallbackId !== undefined && typeof video.cancelVideoFrameCallback === "function") {
      video.cancelVideoFrameCallback(frameCallbackId);
    }
    video.pause();
    video.removeAttribute("src");
    video.load();
    video.remove();
  };

  const finish = (result: DecodeResult) => {
    if (settled) return;
    settled = true;
    cleanup();
    resolveDecode(result);
  };

  const maybeFinish = () => {
    if (metadataDuration === null || !firstFrameDecoded) return;
    finish({ ok: true, duration_ms: metadataDuration });
  };

  const onLoadedMetadata = () => {
    metadataDuration = durationSecondsToMilliseconds(video.duration);
    if (metadataDuration === null) {
      finish({ ok: false, duration_ms: 0 });
      return;
    }

    if (typeof video.requestVideoFrameCallback === "function") {
      frameCallbackId = video.requestVideoFrameCallback(() => {
        firstFrameDecoded = true;
        maybeFinish();
      });
    }
    try {
      void video.play().catch(() => finish({ ok: false, duration_ms: 0 }));
    } catch {
      finish({ ok: false, duration_ms: 0 });
    }
    maybeFinish();
  };

  const onLoadedData = () => {
    firstFrameDecoded = true;
    maybeFinish();
  };

  const onError = () => finish({ ok: false, duration_ms: 0 });

  const promise = new Promise<DecodeResult>((resolve) => {
    resolveDecode = resolve;
  });
  video.addEventListener("loadedmetadata", onLoadedMetadata);
  if (typeof video.requestVideoFrameCallback !== "function") {
    video.addEventListener("loadeddata", onLoadedData);
  }
  video.addEventListener("error", onError);
  video.addEventListener("abort", onError);

  // The host is an offscreen, aria-hidden container owned by the control-window context.
  videoHost.appendChild(video);
  video.src = url;
  if (mockUnavailable) queueMicrotask(onError);
  else video.load();

  return {
    promise,
    cancel: () => {
      if (settled) return;
      settled = true;
      cleanup();
      resolveDecode({ ok: false, duration_ms: 0 });
    },
  };
}

async function probeItem(videoHost: HTMLElement, request: MediaProbeRequest, item: MediaProbeItem, platform: "windows" | "other") {
  const convertedUrl = toWebviewUrl(item.url, platform);
  if (!convertedUrl) return;

  let decode: ReturnType<typeof beginDecode> | undefined;
  let result: DecodeResult = { ok: false, duration_ms: 0 };
  try {
    decode = beginDecode(videoHost, convertedUrl);
    const outcome = await withProbeTimeout(decode.promise, browserClock(), MEDIA_PROBE_TIMEOUT_MS);
    if (outcome.status === "resolved") result = outcome.value;
  } catch {
    // A local decoding failure is reported as a failed probe, not a renderer exception.
  } finally {
    decode?.cancel();
  }

  const report = buildProbeReport({
    install_id: request.install_id,
    revision: request.revision,
    path_sha256: item.path_sha256,
  }, result.ok, result.duration_ms);
  try {
    await reportMediaProbe(report);
  } catch {
    // Probe failure is readiness evidence only; it must not surface as a renderer error.
  }
}

export function MediaProbeContext() {
  const videoHost = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = videoHost.current;
    if (!host) return;

    mountedContexts += 1;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const platform = webviewPlatform();
    const enqueue = createProbeQueue<QueuedProbe>(2, async ({ request, item }) => {
      if (!disposed) await probeItem(host, request, item, platform);
    });
    const enqueueRequest = (request: MediaProbeRequest) => {
      enqueue(request.items.map((item) => ({ request, item })));
    };

    const setup = async () => {
      let stop = () => {};
      try {
        stop = await subscribeMediaProbeRequest((payload) => {
          const request = parseProbeRequest(payload);
          if (request) enqueueRequest(request);
        });
      } catch {
        // Pending probes can still be recovered even if event subscription is unavailable.
      }
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;

      try {
        const pending = await pendingOnceForMountedContext();
        if (disposed) return;
        for (const request of parsePendingProbeRequests(pending)) enqueueRequest(request);
      } catch {
        // A missing native command leaves readiness unknown; do not invent a result.
      }
    };

    void setup().catch(() => {
      // A failed event subscription is contained; a later renderer mount can retry.
    });

    return () => {
      disposed = true;
      unlisten?.();
      host.replaceChildren();
      releasePendingContext();
    };
  }, []);

  return (
    <div
      ref={videoHost}
      aria-hidden="true"
      style={{
        position: "fixed",
        left: "-10000px",
        top: "-10000px",
        width: "1px",
        height: "1px",
        overflow: "hidden",
        opacity: 0,
        pointerEvents: "none",
      }}
    />
  );
}
