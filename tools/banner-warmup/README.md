# Banner warm-up clip

`apps/desktop/src/assets/overlay/banner-warmup.webm` is an original, synthetic clip made only for the overlay's
one-shot media warm-up (`apps/desktop/src/bannerWarmup.ts`). It contains no third-party or pack material.

- Format: WebM, VP9 colour stream plus a VP9 alpha stream in BlockAdditional (AlphaMode 1); 1280x480, 30 fps,
  60 frames (2 s), about 15 KB. Content: black with a faint moving bar; alpha is near zero (max 23/255).
- SHA-256: `b61694603c69e768296d48bc2128ce81216d679268ed0c9973b86f1d26a6025d` (pinned in `apps/desktop/tests/banner-warmup.test.ts`).
- Generated with `node tools/banner-warmup/gen-warmup.mjs <out.webm> 1280 480 60`. The script encodes the two
  streams with WebCodecs in headless Microsoft Edge (`C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe`,
  a throwaway profile, and a blank page the script serves on loopback, because WebCodecs needs a secure context).
  It then writes the EBML/WebM itself. Encoder output is not bit-reproducible across Edge versions; the
  pinned hash identifies the shipped file.
- Why it exists: in game, the first banner after each Kohbor start dropped 7-11 frames with 287-300 ms stalls, and
  later banners were clean. Warming the video path once with this clip made the first banner clean, 0/96 and 0/106,
  in two restarts (native measurement 2026-11-28; owner chose CSP `media-src 'self'`).
