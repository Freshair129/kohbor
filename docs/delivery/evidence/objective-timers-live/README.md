# ALERT-008 / UI-015 live evidence (owner-observed)

Source: integ `9c960135c60f4622fcbb3583912bddb9bedbc795`, which contains `dcb92be` (card under RISK, app locale)
and `7e9bc4f` (fixed dark HUD palette).
Executable: sha256 `968475e1ed1120e0c58fc6c0a5ea7d3ef3a43776e3756c7a719e1238191f10c6`. The file contains `9c96013`.
Receipt: [owner-live-9c96013.receipt.json](owner-live-9c96013.receipt.json).

The receipt is PASS for its named scenario only: card colour, position under RISK, Thai copy, rune timings and
tone outcomes. It does not verify ALERT-008 or UI-015 as a whole. Both stay NOT_RUN at requirement level.

## Covered (owner-observed and logged)

- **Owner's words** (relayed by the lead session, 2026-11-22): “สีกับตำแหน่งโอเคแล้ว” (colour and position OK now).
  The card is dark, centred under RISK, with Thai copy.
- **App log snapshot:** sha256 `e695d192e8269407ebb067b4469fd292ff94c73f348a0f4daf2294d8dce6963e`, 278 lines, kept
  outside the repo, with no identity strings.

| Item | Result |
| --- | --- |
| Clock covered | 1065 s to 2160 s (about 17:45 to 36:00) |
| `objective_alert_shown` | 31 lines |
| Power rune | 30 s and 15 s pre-warnings and a spawn every 2 minutes |
| Bounty rune | spawns at 20:00, 24:00, 28:00, 32:00 and 36:00, logged in `fields.kinds` |
| Wisdom rune | spawns at 21:00 and 35:00, and in the 28:00 batch |
| Tone outcome | 29 `played`, 2 `deferred_played`, 0 `suppressed` |
| Warn or error lines | none |

## Not covered (still needed for the requirements)

- The Tormentor first spawn and the 2:00 water runes: the app started at clock 17:45.
- A kill or event video banner over the card.
- The layout editor in the native app (drag, reset, a saved custom layout).
- Lead-time changes.
- The English UI.
- Whether each tone sounds distinct by ear.

This is owner-observed evidence, not an agent approval and not a signed acceptance.
