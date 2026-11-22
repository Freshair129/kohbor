# ALERT-008 / UI-015 live evidence (owner-observed)

Source: integ build `9c960135c60f4622fcbb3583912bddb9bedbc795` (log header commit `9c96013`), which contains
`dcb92be` (card under RISK, app locale) and `7e9bc4f` (fixed dark HUD palette).
Receipt: [owner-live-9c96013.receipt.json](owner-live-9c96013.receipt.json), status INDETERMINATE.

## What the owner observed

- Owner's words (relayed by the lead session, 2026-11-22): “สีกับตำแหน่งโอเคแล้ว” (colour and position OK now).
- The card is dark, centred under the RISK meter, with Thai copy.

## What the log shows

App log snapshot: sha256 `e695d192e8269407ebb067b4469fd292ff94c73f348a0f4daf2294d8dce6963e`, 278 lines, kept outside the repo.

| Item | Result |
| --- | --- |
| Clock covered | 1065 s to 2160 s (about 17:45 to 36:00) |
| `objective_alert_shown` | 31 lines |
| Power rune | 30 s and 15 s pre-warnings and a spawn every 2 minutes |
| Wisdom rune | spawns at 21:00 and 35:00, and inside the 28:00 batch |
| Bounty rune | inferred from batched spawns at 20:00, 24:00, 28:00, 32:00 and 36:00 |
| Tone outcome | 29 `played`, 2 `deferred_played`, 0 `suppressed` |
| Warn or error lines | none |

## Why this is not PASS

- The executable the owner ran was not hashed. The run is bound to source only through the log header.
- These were not covered:
  - Tormentor and the 2:00 water runes: the app started at 17:45.
  - A kill or event banner over the card.
  - The layout editor in the native app.
  - A saved non-default layout.
  - Lead-time changes.
  - The English UI.
  - Per-objective tone distinctness by ear.
- A batched alert logs only one `kind`, so bounty is inferred, not named in the log. This is a logging gap to fix.

This is owner-observed evidence, not an agent approval and not a signed acceptance.
