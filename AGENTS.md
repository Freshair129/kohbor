# Agent instructions for Kohbor / เกาะเบาะ

## Mission

Build Kohbor / เกาะเบาะ, the desktop companion described in this package. The product owner has selected the name; do not reopen naming as a delegated task. Design an original presentation identity and use functional module names internally. Preserve the target behavior and consent boundaries.

## Read order and authority

Read README, docs/product/PRD.md, docs/requirements/SRS.md, docs/governance/DOCUMENT-CONTROL.md and docs/governance/DECISION-LOG.md, then the relevant design, use cases and verification procedures. Read applicable views in docs/diagrams/README.md with their guard notes; edit Mermaid sources and regenerate images together when a governing requirement changes. PRD/SRS define one baseline; contracts/requirements.json is its structured representation. A mismatch is a defect to repair, not a competing authority.

- Separate inherited targets, proposed schemas, implementation choices and unverified assumptions.
- A conflict is a decision to resolve, not permission to choose whichever behavior is easiest.
- Open decisions block only their named dependencies. Continue independent authorized work.
- Do not add cloud data, paid services, telemetry, game automation, extra platforms or product features because a dependency makes them convenient.
- Unknown is a real state. Never fill production with fixture data, fabricated readiness, unobserved enemy fields, or default values masquerading as measurements.

## Identity

The owner-selected display names are Kohbor and เกาะเบาะ, recorded in contracts/product-identity.json. This records the name decision only: DEC-NAME remains delegated for the unfrozen technical identity. Before creating the application skeleton, complete that existing manifest and freeze app/package IDs, data directory, URI scheme and signature/hash namespaces in coordination with their interface and trust contracts. Determine an update origin only when the corresponding service is specified; do not infer a domain or endpoint from the product name. contracts/product-identity.example.json illustrates the remaining fields and is not a completed identity.

Original logo, character, palette, typography and other visual design remain delegated under DEC-VISUALS. Any later owner-approved display-name change is independent of stable technical IDs. Do not bring source-brand assets, secrets, real-person voices or legacy trust roots into the new product. Compatibility with an older product is not promised.

## Work policy

Use one owned checkout and a normal feature branch by default. One active writer owns each mutable file area. Read-only reviewers may share it. Use isolated worktrees only for a demonstrated concurrent write requirement and record owner, task and disposal conditions. Never create a worktree per small task or per review.

Keep shared contracts and generated files under one integration owner. Before editing, check working-tree status, existing branches and active task ownership. Preserve others' work. Do not reset, force-push, delete a checkout, or rewrite shared history as a routine recovery.

## Implementation and evidence

Implement dependency-ordered vertical slices from docs/delivery/SDP.md. Pin actual dependencies and toolchains after the feasibility spike; do not label an untested model or patch dataset compatible.

For each slice, update docs/requirements/RTM.csv through its registry: product need IDs, requirement IDs, design components, real implementation/test references and evidence receipts. Report changed modules, actual commands/results, observed limits, decisions made, open dependencies and next concrete work. Source checks, synthetic tests, Windows native tests, live matches, acoustic timing and resource/FPS measurements are distinct evidence classes.

Do not mark SKIP, unavailable hardware, a screenshot or a synthetic fixture as PASS for another evidence class. Bind every acceptance result to exact source revision and artifact hash. Do not weaken a threshold after seeing a failure.

No publish, production deployment, payment activation, release signing or destruction of user data follows merely from a request to build the new application. Prepare concrete reviewable artifacts and use the authorization available in the active task.
