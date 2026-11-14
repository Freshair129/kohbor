---
domain: commerce
tier: B
card_version: 3
verified_at_commit: d94cb96
verified_on: 2026-11-14
verified_by: "claude-code governance session, seeded from DEC-COMMERCE-INTENT and the documents named below; agent work, not human review"
---

# Commerce: store, wallet, payments, entitlement

A map, not evidence. The delivery design exists as a proposal; nothing is built.

## 1. Purpose and boundary

A design of pack delivery exists as an agent proposal, not owner approved (docs/design/STORE-PACK-DELIVERY.md). No requirement is applied, no decision about it is closed, and payments are not designed. Nothing may be built, hosted, priced or paid for from this card or from that document. Later owner of: store service, catalog, wallet, entitlement. Needs `account` (who buys), `packs` (what is sold, signature) and `release-ops` (hosting).

## 2. Owner intent (a statement, not a design)

- 2026-11-13, owner statement: Kohbor has a store system (none exists in the repository) and the owner plans to produce announcer packs in volume to earn revenue; rights and licensing are handled by the owner's rights team, an owner statement that is unverified and not a rights grant (DEC-COMMERCE-INTENT).
- 2026-11-14, owner request ("เขียนเอกสาร", relayed by the coordinating session): write the store and pack delivery design as a document only. This authorised that one document. It authorised nothing else and did not widen the agent policy for this domain.
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

DEC-ACCOUNT-ORIGIN (production hosting; the alpha/beta hosting is recorded as provisional on the owner's statement of 2026-11-14, see docs/design/BACKEND-STACK.md), DEC-PACK-CRYPTO (signed packs), DEC-PRIVACY-LEGAL, DEC-PREMIUM (deferred), the nine open store decisions the design adds (listed in its section 13), the owner's rights documents for any pack that is sold or distributed, and the AGENTS.md rule on real-person voices and source-brand assets. The build order and which step waits for which decision is in the design, section 12.

## 5. Questions for the owner (all are open decisions)

DEC-STORE-SCOPE (what is sold, account to buy, relation to premium), DEC-STORE-HOSTING (provisional: the hosting part is decided for the alpha, R2 versus Supabase Storage and production hosting are open), DEC-STORE-OFFLINE-POLICY, DEC-STORE-PAYMENT-PROVIDER, DEC-STORE-CURRENCY-TAX, DEC-STORE-REFUND, DEC-STORE-REVENUE-SHARE, DEC-STORE-RECORDS, DEC-STORE-TAKEDOWN. Recommended defaults, where the design has one, are in its section 13. Q-COM-1 in [state.json](state.json) is the follow-up for governance.

## 6. Artefacts

Governing proposal: [STORE-PACK-DELIVERY](../../design/STORE-PACK-DELIVERY.md). Hosting reference: [BACKEND-STACK](../../design/BACKEND-STACK.md). Reads: the documents cited in section 3 (see [state.json](state.json)).

## 7. Who may write, how to claim, how to ask

Owned by `governance`. The agent policy for this domain stays questions-only (docs/governance/AGENT-REGISTRY.json); the 2026-11-14 design was a one-time owner authorisation for that document. Further design work needs the owner to say so, and then the policy may move to design-only. An agent may not price, build, host, create a provider account or key, or decide rights and licensing. Ask the owner for anything about price, provider, hosting or what is sold.
