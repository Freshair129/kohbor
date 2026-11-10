---
title: "User Acceptance Test Plan"
doc_id: "UAT-PLAN"
status: "draft"
version: "0.3.3"
updated: "2026-11-10"
owner_role: "UAT operator — unassigned"
approval_status: "not_approved"
---

# User Acceptance Test Plan

## 1. Conditions and recording

These player tasks validate [PRD needs](../product/PRD.md) against the [SRS](../requirements/SRS.md). They do not replace [Test Plan](TEST-PLAN.md) negative/race suites or [NFR](NFR.md) measurements. Record build/profile/environment, actions, expected/observed outcome, evidence and status for each task. All start **NOT_RUN**. Use authorized media/test accounts and an explicitly consented microphone; destructive checks use isolated test data.

## 2. Player tasks

### UAT-01 — Start independently

**PN-001; JRN-001; CORE-001, CORE-003, CORE-004, CORE-005.** Prepare a fresh installed test profile with network off and unrelated game configuration backed up. Launch signed out, inspect setup, reconnect and optionally sign out again. Observe available local controls, honest unavailable services and preserved unrelated configuration. **Evidence:** startup receipt, configuration comparison and status capture.

### UAT-02 — Recognize unavailable information

**PN-002; JRN-001; DATA-001–006, VISION-001–003, QUALITY-004.** Prepare a supported session plus controlled source-gap cases. Observe real zero versus unknown, then interrupt vision or valid updates. Confirm stale/unavailable labels, no invented enemy values and independent valid telemetry continuing. **Evidence:** timestamped source/UI trace and certified-class list; label injected cases separately from live observations.

### UAT-03 — Hear justified warnings

**PN-003; JRN-002; ALERT-001–007, QUALITY-001, QUALITY-005.** Prepare verified critical media and admitted motion scenarios after the motion decision closes. Observe qualifying warning, continuing absence, time decay and confirmed reappearance. Expect no threshold-only warning, repeated episode or decay-only correction. **Evidence:** event/admission receipts and synchronized audio; link the separate full acoustic/quality qualification.

### UAT-04 — Install and activate media

**PN-004; JRN-003; MEDIA-001–008.** Prepare authorized valid and corrupted packages. Install while idle and during an active match/media lease, then preview the admitted event. Observe installed versus active/deferred status, matching audio/video, preserved selection on rejection and no substitute for unmapped media. **Evidence:** installation receipts, revision hashes and recorded playback.

### UAT-05 — Use the control window and overlay

**PN-005; JRN-008/JRN-011; UI-001–006, CORE-002, QUALITY-002–003.** Prepare the target display/game configuration. Navigate with keyboard, select a hero, expand details, press Escape, reposition/mute the overlay and return to play. Observe visible focus, stable map, truthful compact cards and passive click-through without focus theft. **Evidence:** native interaction recording plus linked resource/FPS receipts.

### UAT-06 — Ask privately by voice

**PN-006; JRN-004; VOICE-001–007, QUALITY-006–007.** Prepare an admitted local model and microphone. Verify default-off, opt in, hold/release PTT in Thai/English, remap and trigger an urgent interruption. Observe local answer or precise unavailable/preparing state, stopped obsolete speech and no resumed cancelled turn. **Evidence:** sanitized lifecycle/no-egress receipts and language/performance report; do not retain runtime transcript/audio merely for this task.

### UAT-07 — Control manual advice context

**PN-007; JRN-005; ADVICE-001–004.** Prepare local advice and, only if scheduled, a controlled cloud endpoint. Request local advice; inspect separate field/purpose consent before enabling cloud; withdraw consent during a pending request. Expect local/preemptible behavior and no late cloud result or forbidden context. **Evidence:** request-generation log and sanitized outbound-field comparison.

### UAT-08 — Review and forget memory

**PN-008; JRN-010; MEM-001–005.** Prepare known complete/partial test sessions after eligibility is defined. Open history and summary, follow evidence/timestamps, clear derived memory and restart. Observe at most three supported summary points, no invented outcomes, forgotten aggregates staying absent and separate history preserved. **Evidence:** before/after snapshot and journal digests; no real user-history deletion.

### UAT-09 — Share and leave deliberately

**PN-009; JRN-006; PEER-001–009; extension.** Prepare two consenting devices in admitted same-scope conditions. Join with the checkbox, observe vitals, interrupt transport and leave during reconnect. Expect freshness expiry, no unauthorized fields, immediate local withdrawal and no late resurrection or autojoin. **Evidence:** both screens, membership/generation receipts and persistence inspection.

### UAT-10 — Invite without implicit sharing

**PN-010; JRN-007; SOCIAL-001–003; extension.** Prepare reviewed social services/test accounts. Establish friendship, change presence, send/accept an invitation and test expired authority. Friendship alone must share no gameplay; joining requires fresh sharing consent. Failure must not silently enter a guest route. **Evidence:** sanitized authorization/transaction receipts and both user-visible outcomes.

### UAT-11 — Save a truthful highlight

**PN-011; JRN-009; OBS-001–005, VISION-004.** Prepare authenticated local OBS and approved recording configuration. Start explicitly, request an event clip, repeat with short history, then stop/restart OBS. Inspect actual saved files and intervals: target 30 seconds before/15 after, truthful partial coverage and isolated recorder failure. **Evidence:** clip/index/save receipts and recording-profile acoustic/FPS/resource measurements.

### UAT-12 — Update and retain control

**PN-012; UPDATE-001–004, QUALITY-008; PREMIUM-001–003/UI-007 deferred.** Prepare a signed test candidate and recovery route. Request update during gameplay, then at a safe boundary; confirm deferred installation and usable data afterward. If premium is separately scheduled, test lease loss/regain without map disruption or safety gating. **Evidence:** artifact/signature/install/recovery receipts; premium outcomes remain NOT_RUN until its prerequisites exist.

## 3. Scope gates and acceptance record

Limited-demo acceptance uses only its declared subset and evidence; it never marks full MVP passed. Full MVP requires every applicable core task, local voice/manual advice/memory, local highlights and the complete NFR matrix. Peer/social tasks are extension gates. Premium checks remain deferred unless explicitly included. Missing prerequisites yield SKIP/INDETERMINATE, never a successful capability claim; unavailable-state behavior may be accepted separately.

Record defects with task/requirement IDs and replay the affected tasks after fixes. Keep failed receipts and known limitations. Summarize task outcomes in the RTM before any acceptance decision.

| Acceptance field | Value |
| --- | --- |
| Candidate version / source / artifact hash | __________ |
| Scope and excluded capabilities | __________ |
| Result and unresolved defects | NOT APPROVED — __________ |
| UAT operator / date | __________ |
| Verification reviewer / date | __________ |
| Product owner acceptance / date | __________ |
| Release decision, if separately authorized | __________ |

Blank fields are intentionally unsigned. Preparing this plan grants no acceptance or release approval.
