---
domain: commerce
tier: B
card_version: 1
verified_at_commit: 7ad4c7f
verified_on: 2026-11-13
verified_by: "claude-code governance session, seeded from DEC-COMMERCE-INTENT and the documents named below; agent work, not human review"
---

# Commerce: store, wallet, payments, entitlement (seed card)

A map, not evidence. Almost empty on purpose.

## 1. Purpose and boundary

Not designed. No requirement, no design document, no design decision exists. Nothing may be built, hosted, priced or paid for from this card. Later owner of: store service, catalog, wallet, entitlement. Needs `account` (who buys), `packs` (what is sold, signature) and `release-ops` (hosting).

## 2. Owner intent (a statement, not a design)

- 2026-11-13, owner statement: Kohbor has a store system (none exists in the repository) and the owner plans to produce announcer packs in volume to earn revenue; rights and licensing are handled by the owner's rights team, an owner statement that is unverified and not a rights grant (DEC-COMMERCE-INTENT).
- It authorises no payment provider, price, wallet, catalog, account requirement for buying, hosting, paid service or sale of any pack (DEC-COMMERCE-INTENT).
- Engineering finishes pack production tooling first (DEC-COMMERCE-INTENT, safe interim).

## 3. Facts found in the repository

- Store, wallet and payments are a later service that needs separate design and operational authorization (docs/product/FEATURE-CATALOG.md, capability table).
- A "Control: Store" destination (catalog, wallet, inventory, history) stays visible with an honest unavailable state and never gates local gameplay (docs/design/UX-SPEC.md section 2).
- No code exists beyond that: CMP-ENTITLEMENT is not scheduled (docs/design/REMAINING-SYSTEM-DESIGN.md section 4; DEC-PREMIUM deferred).
- New account or payment services need a product or contract decision, and no paid service follows from a build request (docs/governance/DECISION-LOG.md; AGENTS.md).
- Integrity is not purchase entitlement; a local import never gains store authority (MEDIA-008).
- Recommended default for a later store: signature plus entitlement, no encryption (docs/design/PACKAGE-FORMAT-REVIEW.md section 7).
- The Kohbor account service shares nothing with the owner's earlier product (docs/design/ACCOUNT-IDENTITY-REVIEW.md section 2.2).

## 4. Blocked by

DEC-ACCOUNT-ORIGIN (hosting, operator, any paid service), DEC-PACK-CRYPTO (signed packs), DEC-PRIVACY-LEGAL, DEC-PREMIUM (deferred), the owner's rights documents for any pack that is sold, and the AGENTS.md rule on real-person voices and source-brand assets.

## 5. First questions for the owner (each would become a DEC id when asked)

What is sold (packs only?); is an account required to buy (DEC-SIGNIN-OPTIONAL keeps guests local); payment provider and operator; currency, refunds, tax; creator revenue share; relation to premium (DEC-PREMIUM).

## 6. Artefacts

None owned. Reads: the documents cited in section 3 (see [state.json](state.json)).

## 7. Who may write, how to claim, how to ask

Owned by `governance` until the owner authorises a design. An agent may only draft neutral questions for the owner from the repository facts in section 3. It may not design, price, build, host, create a provider account or key, or decide rights and licensing. Ask the owner for anything about price, provider, hosting or what is sold.
