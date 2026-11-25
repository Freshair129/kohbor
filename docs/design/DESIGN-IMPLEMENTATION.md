# Analytics layout implementation notes

**Decision:** DEC-ANALYTICS-LAYOUT, approved by the product owner on 2026-11-25. The detailed visual and interaction source remains `F:\kohbor-coord\design\analytics-layout-20261125\DESIGN.md`; its `prototype.html` is a visual reference, not application evidence.

## Slice ownership and evidence

The current work implements the pane model and controls, square-map/timeline composition, shell-header toolbar, Replay/Analytics stat icons, shared hero-card display preference, replay status tray, selection actions and compact card density. `apps/desktop/src/live/HeroCard.tsx` and `heroCard.css` belong to OV and were not edited. Live wiring is a follow-up after that layout merges.

Pure model tests cover the five target window sizes and map-side calculations, size-class storage/reset, toolbar collapse order, checkbox reflow, modifier priority/overflow, quote composition and compact density. They do not prove rendered geometry or Windows native behavior. The lead owns render/typecheck and any app-based acceptance.

## Live hero-card wiring after OV merges

1. Import `readHeroCardDisplay`, `writeHeroCardDisplay`, `visibleStatKeys` and `HERO_CARD_DISPLAY_EVENT` from `apps/desktop/src/shared/heroCardDisplay.mjs`. Use the shared preference to filter Live stat cells; do not mirror the thirteen booleans in Live.
2. Keep `HeroCardDisplaySettings` and `HeroCardDisplayControls` as the shared Settings/quick-access surface. Add the Live quick button to the OV-owned card-area header only after OV's merge; changes write immediately through `writeHeroCardDisplay`.
3. Listen for the same-window `HERO_CARD_DISPLAY_EVENT` and the cross-window `storage` event so the Live renderer refreshes without a reload. The storage key is `kohbor.hero-card.display` and stores presentation preference only.
4. GPM/XPM currently applies to Live only. The status checkbox currently affects Replay only because Live has no admitted status producer. Keep both states explicit until their data contracts change.
5. Do not change `HeroCard.tsx` or `heroCard.css` as part of this task. Keep the unresolved permanent/aura/stack data note and do not derive Duel-win or tower buffs in either renderer.
