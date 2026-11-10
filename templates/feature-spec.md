# Feature specification template

Metadata: feature_id; version; status; owner_role; target_milestone; decision_basis.

## User goal and scope
Actor, job, required outcome, included behavior, explicit non-goals.

## Dependencies and authority
Product need IDs; SRS requirement IDs; allocated design components; related diagram IDs; prior decisions; contracts; source/catalog revisions; permission/consent.

## Inputs and outputs
Field/type/unit/required/nullability; source and admission; timing/freshness; output ownership.

## Behavior
Trigger; preconditions; ordered rules; state transition; positive path; no-data path; invalid/stale path; timeout/retry; cancellation/late result; offline; restart; concurrency/idempotency.

## User interface
Screen/entry/action/exit; loading/empty/unknown/stale/error/denied/success; keyboard/focus; copy; accessible status.

## Data and resources
Storage/lifetime/delete; allowed recipients; secrets; log fields; CPU/RAM/latency budget.

## Acceptance
Given / When / Then, exact boundary values, fixtures, expected errors, evidence class, pass threshold.

## Open decisions
Decision ID, owner role, deadline milestone, blocked scope, safe interim state, closure evidence.
