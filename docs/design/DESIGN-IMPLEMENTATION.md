# Analytics layout implementation notes

**Decision:** DEC-ANALYTICS-LAYOUT, approved by the product owner on 2026-11-25. The detailed visual and interaction source remains `F:\kohbor-coord\design\analytics-layout-20261125\DESIGN.md`; its `prototype.html` is a visual reference, not application evidence.

## Implemented wiring

The renderer uses the pane model and controls, square-map/timeline composition, shell-header toolbar, Replay/Analytics stat icons, the shared hero-card display preference, replay status tray, selection actions and compact card density. The layout decision is recorded in `docs/governance/DECISION-LOG.md`.

Replay status rendering consumes `heroes[].mods_long` and `heroes[].mod_stacks` from schema 3.2. It shows permanent effects with `∞`, active aura intervals, positive stack counts and a readable unit/team source; `subtle` modifiers are hidden. Permanent buffs occupy the left end of the tray. Icon resolution uses only local icons and retains its category glyph fallback. Schema 3.0 and 3.1 bundles keep an ⓘ note that the replay needs a re-parse with the new engine. The engine spec also records that short tower and fountain auras of 90 seconds or less can be absent from both modifier lists.

Live own and compact cards read the same preference from `apps/desktop/src/shared/heroCardDisplay.mjs`. They refresh from `HERO_CARD_DISPLAY_EVENT` and cross-window `storage` events, omit unchecked stat cells and status placeholders, and suppress derived regeneration labels when that group is unchecked. Existing 12 px Live text and field-card sizing rules remain in `heroCard.css`; the CSS stat grid reflows the cells that remain. Live still has no admitted status producer, so its status group controls only the existing placeholder row.

The setting remains presentation-only. `HeroCardDisplaySettings` and `HeroCardDisplayControls` own the shared Settings and Analytics surfaces; no separate Live copy of the thirteen booleans is maintained.

## Evidence boundary

Pure model tests cover the five target window sizes and map-side calculations, size-class storage/reset, toolbar collapse order, checkbox reflow, modifier priority/overflow, quote composition and compact density. Source and Node fixture checks do not prove rendered geometry or Windows native behavior. Those acceptance checks remain separate and are not implied by this wiring.
