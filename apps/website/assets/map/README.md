# field.png

A 256 x 256 RGBA field traced from the Dota 2 map overview. The stage (`js/stage.js`) reads it to place and shade the point cloud.

- Source: `apps/desktop/src/assets/dota/6989/map.png` in this repository. It was exported from the owner's installed Dota 2 (game build 6989, `materials/overviews/dota_737_psd_28d44696.vtex_c`), read only, with no download. See that folder's `provenance.json`.
- Owner decision, 2026-11-23: use the real map for the website hologram ("เอา map Dota จริงๆมาแกะ"). The original overview image is not shipped here. Only this derived field is.
- Channels, each computed per cell from the 900 x 900 map area (pixels 62 to 962):
  - R: brightness × 1.35
  - G: the share of water-coloured pixels (green and blue both above 1.3 × red, and dark)
  - B: local brightness texture, as a standard deviation × 5 over the cell plus 3 px. Trees and cliff edges score high.
- The bake script used a stdlib PNG reader. Re-bake only if the source overview changes.
