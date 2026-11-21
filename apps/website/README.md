# Kohbor / เกาะเบาะ — website design prototype

Status: design exploration on the owner's request (2026-11-21). Design and copy are not reviewed or approved.
Deployed to Vercel on the owner's instruction the same day; see "Deployment" below.
Built first in the coordination ledger (`F:\kohbor-coord\design\website-20261121\`, claim `ui-overlay__website-design`), then moved
here, to `apps/website/`, on the owner's instruction "commit push pr" (2026-11-21; the owner chose this location). Being in the
repository does not put a public website in the product scope; see the open decisions at the end.

## Run it

```
node serve.mjs
```

Then open `http://127.0.0.1:4173/`. The server binds to 127.0.0.1 only and has no dependencies.
Opening `index.html` from disk does not work (ES modules and video ranges need HTTP).

The film videos (`assets/film/kohbor-film.mp4` and `.webm`, about 21 MB) are not in git; the owner chose on 2026-11-21 to keep
them out. Only `assets/film/poster.jpg` is committed. Without the videos the film chapter falls back to drawing the film live on a
canvas. To render the videos (after a fresh clone, or after changing `js/film.js` or `js/stage.js`), open
`http://127.0.0.1:4173/studio.html` and press **Render and save** (or add `?auto=1`). It encodes frame by frame with WebCodecs
(a Chromium browser is needed) and writes `assets/film/`.

## Deployment (Vercel)

The owner instructed "Deploy to vercel" on 2026-11-21. Project `kohbor-site-prototype` in the owner's Vercel scope, linked from this
folder (`.vercel/`). The command run was a preview deploy (`vercel deploy --yes`), but Vercel assigns a project's **first** deployment
to production, so the page is **public** at `https://kohbor-site-prototype.vercel.app`. That outcome was not the stated plan (preview);
the owner was told, and decided the same day to keep it public ("เปิดไว้", 2026-11-21). That decision covers this prototype URL
only: it is not an approval of the design or copy, and it does not set a domain or an official website.

- Deployed: the page, `css/`, `js/`, `assets/`, `vendor/three.module.min.js`. Not deployed (`.vercelignore`): this README, `serve.mjs`,
  `studio.html`, the two muxers, `.env*`.
- `vercel.json` sends `X-Robots-Tag: noindex, nofollow` on every response; the page also carries a robots meta tag.
- Checked after deploy: every page file returns 200, the excluded files return 404, video byte ranges return 206, and the page renders
  with the stage and the film playing in the built-in browser pane.
- The live deployment was made from the ledger folder, which still holds the Vercel link (`.vercel/`) and the rendered videos.
  The link is not in git. To deploy from `apps/website/`: render the videos first (see "Run it"), then `vercel link` to the
  project `kohbor-site-prototype`. A deploy without the videos would replace the film with the live canvas fallback.
- To update: `vercel deploy` creates a preview URL; `vercel deploy --prod` replaces the public page. Both are the owner's call.
- `vercel link` can write a `.env.local` holding a token; `.gitignore` here ignores `.env*`. The one it wrote in the ledger folder
  was deleted at once.

## What is here

| Path | Role |
| --- | --- |
| `index.html`, `css/site.css` | The page: ten chapters, Thai first with an English switch |
| `js/stage.js` | The 3D stage (three.js, one point field, markers, map frame, orb). Deterministic: a frame is a function of its params |
| `js/main.js` | Scroll score: one blended scene key per frame, per-chapter values, interactions |
| `js/film.js`, `studio.html` | The 20 s film: the same stage driven by time; the studio encodes it to MP4 (H.264) and WebM (VP9) |
| `assets/film/` | Poster (in git) and the rendered film (1600 x 900, 30 fps, silent; not in git, rendered by `studio.html`) |
| `assets/mark.svg` | Copy of the app mark `apps/desktop/src/assets/brand/kohbor-rail-mark.svg` |
| `vendor/` | three.js 0.170.0 (MIT), mp4-muxer 5.2.1 and webm-muxer 5.1.3 (MIT; studio only), from npm. The only network request is Google Fonts for IBM Plex |

## Design decisions (mine, for the owner to accept or change)

1. **The page obeys the product's own rules.** No invented number, testimonial or download. What does not exist yet is drawn dashed
   and says so (the download button is disabled with "not open"; the three acceptance thresholds show "Measured: — not run yet").
2. **Three rooms, each a flat field of a brand colour**: Charcoal (in the match), Bone (what is known or promised), Barber Red
   (what Kohbor refuses to do). Barber Red is a fill only, with Bone as its only ink, as the tokens require. The ink of the 3D
   scene flips exactly on the line where two rooms meet.
3. **Line style is meaning**, as in the app: solid = seen/real, dashed = missing/not yet real, hatched = stale, double ring = you.
4. **One continuous shot.** One point field becomes an island, a flat map, three mixer bands, a voice sphere and a calm sea.
   "Koh" also reads as island; the island is original geometry (a sum of Gaussian hills), not a game map.
5. **A landing page, not the app.** A thin top header (brand, chapter links, language) that takes the ink of the room under it,
   and a 2 px scroll line. The first draft framed the page with the app's two side rails and a frame-rate badge; the owner
   rejected that on 2026-11-21 ("a landing page is not an application"), so the app shell is not borrowed anywhere on the page.
6. Headline "คุณขับ เราเกาะเบาะ / You drive. We ride along." is a proposal, not an approved tagline.
7. Type and colour are the Command Deck tokens only (IBM Plex Sans Thai, IBM Plex Mono for numerals). No new token was added.

## Where the copy comes from

Every product statement paraphrases the baseline, nothing is added: PRD sections 1, 4 (non-goals), 5 (rules 1, 4, 5), 6, 7, 8;
Command Deck design notes sections 1 and 3 (shapes, state wording); FEATURE-CATALOG (event tunables, silent when unmapped).
The line about win rate restates PRD section 1 ("no demonstrated win-rate effect").

## Checked, and not checked

Checked in the built-in browser pane (Chromium) on this machine, 2026-11-21:
- Every chapter renders; state switcher, mixer, hold-to-ask, language switch, motion switch and film playback work.
- No horizontal page scroll and no clipped chapter content at 375 x 812, 1280 x 720, 1366 x 768, 1440 x 900 and 1920 x 1080 (DOM measurement).
- 60 fps on the hero at 1440 x 900 while the pane was in view (first draft, before the header change). Later checks ran with the window in the background, so their frame rates mean nothing.
- After the header change the same five sizes were measured again: no clipped content, nothing under the header, no horizontal scroll.
- In `apps/website/` without the film videos (as after a clone): the first run showed that the live-film fallback never started,
  because the source error fired before the script attached its listener. `js/main.js` now also checks the video state at load.
  After that fix the live canvas film plays and the video element is hidden.

Not checked: Firefox, Safari, a real phone, touch dragging, a screen reader, colour-vision simulation, low-end GPUs, the WebGL-unavailable
path (coded, not exercised), and reduced motion with the OS setting (only the page switch was tried).
Thai wording has not been read by anyone but the author. The film files total about 21 MB and are not optimised for delivery.

## Owner decisions this needs before it could go anywhere

- Whether a public website is in scope at all (PRD non-goals exclude unapproved public services), and the domain (none is implied here).
- The tagline, and the Thai and English copy.
- Trademark wording for the Dota 2 notice (rights and legal questions stay with the owner).

Decided by the owner on 2026-11-21: keep the prototype URL public ("เปิดไว้"); move the prototype into the repository at
`apps/website/`; keep the rendered film videos out of git.
