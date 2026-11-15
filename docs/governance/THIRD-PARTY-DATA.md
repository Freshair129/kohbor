# Third-party data files

Data files copied into the repository from outside sources. Each entry records origin, hash, use, and what is NOT established. Rights are never decided by engineering.

## apps/desktop/src-tauri/data/item-prices.json

- Content: 294 entries, internal item name (no `item_` prefix) to shop cost in gold, from 1 to 7650.
- Use: derived net worth in the match HUD (`hud.rs`: gold plus the shop cost of every named item in the own items block). Compiled in with `include_str!`; no runtime fetch.
- Origin: copied byte for byte on 2026-11-15 from the owner's earlier project (G-Maiden) file `src-tauri/data/item-prices.json`. That project's own provenance record lists the upstream as an OpenDota "dotaconstants" snapshot; upstream revision and retrieval date were not recorded there either.
- SHA-256: `1d9fdd96788b91ed4cb1b8db140f8575540916aa654d15dbc859acd1b7259fd3`.
- Authorisation: the product owner authorised the copy in chat on 2026-11-15 ("ลอกมาเลยก็ได้ เพราะเกมเดียวกัน"). This is an owner statement, not a licence review.
- NOT established: the licence of the upstream data, the patch the prices belong to (prices change between patches; an item missing from the table makes the HUD net worth approximate and the chip shows a leading `~`), and any right to redistribute the file in a shipped product. Rights stay with the owner's rights process.
- Update rule: replace the file only with a new recorded snapshot (origin, date, hash) and keep the parse test (`price_table_parses_and_resolves_bare_and_prefixed_names`) passing.
