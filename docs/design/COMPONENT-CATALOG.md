---
title: "Kohbor Component Catalog"
doc_id: "COMPONENT-CATALOG"
status: "design_elaboration"
version: "1.0.0"
spec_baseline: "0.3.1 / DEC-BASELINE"
authored: "2026-11-11"
timezone: "Asia/Bangkok"
approval_status: "not_approved"
native_acceptance: "NOT_RUN"
---

# Component Catalog 1.0.0

This catalog specifies reusable visual and interaction contracts for the [Kohbor design system](DESIGN-SYSTEM.md). **DS-Cxx** identifiers identify design components; **CMP-*** identifiers below are the existing canonical engineering components. The catalog does not rename those components or create a new requirement inventory. Every component inherits the fixed-stage, data-truthfulness, typography, and evidence rules of [UX-SPEC](UX-SPEC.md) and [SRS](../requirements/SRS.md).

## Shared contract

The following apply to every component unless a narrower contract follows.

| Concern | Required behavior |
| --- | --- |
| Inputs | Consume admitted read-model values with availability, source, observation age, source generation, revision, and entity/session guards where relevant. A raw parser value or rendered fixture is not authority. |
| Events | Named events below are conceptual user intents. Native authorities validate capability, consent, source, identity, bounds, and current generation. Actual IPC framing/payloads remain DEC-INTERFACES work. |
| Token use | Use generated semantic/component tokens. CSS names in this catalog are actual semantic-token names; do not copy colors into component styles. |
| States | Distinguish empty, loading, live, stale, unavailable, error, user-disabled, and action-disabled. Only relevant states need a visual; unused states must not fabricate a producer. |
| Focus | Keep 2 px visible focus with 3 px offset, preserve native semantics, and retain meaningful labels. Background source updates do not move focus. |
| Targets | 44 px authored default; 36 px compact only in documented dense regions. Native scaled minimum remains unmeasured. |
| Text | 16 px body / 14 px supporting, Thai-safe line height 1.6; wrap prose, truncate only compact identity labels with an accessible full name. |
| Feedback | Explain a failed action next to the affected control; preserve prior valid state. Reserve dialogs for a task that actually blocks local interaction. |
| Evidence | The board demonstrates a fixture contract. It cannot establish native game focus, audio output, OBS behavior, permissions, model quality, or performance. |

Do not render card-wide click handlers around nested controls. Use one named main selection button and separate sibling actions; their hit areas cannot overlap. Do not use opacity to hide the reason an action is unavailable.

## Inventory and traceability

| ID | Component | Canonical engineering component(s) | Primary requirement coverage |
| --- | --- | --- | --- |
| DS-C01 | Application shell and navigation | CMP-READMODEL, CMP-STARTUP | CORE-001, CORE-005, UI-001, UI-002, UI-006 |
| DS-C02 | Header and bounded page section | CMP-READMODEL | UI-002, UI-006 |
| DS-C03 | Button, icon action, and link | CMP-READMODEL | UI-006; consuming action's requirement |
| DS-C04 | Field, checkbox, switch, slider | CMP-READMODEL | UI-006; VOICE-001–VOICE-002; ADVICE-002; PEER-001 |
| DS-C05 | Tabs and filters | CMP-READMODEL | UI-002, UI-006, MEM-004, OBS-003 |
| DS-C06 | Status and provenance | CMP-READMODEL, CMP-INGEST | CORE-005, DATA-001–DATA-006, PEER-005 |
| DS-C07 | Empty, loading, unavailable, and error | CMP-READMODEL | UI-003–UI-004, DATA-002, VOICE-007, OBS-004 |
| DS-C08 | Stat value and meter | CMP-READMODEL, CMP-INGEST | DATA-001–DATA-003, PEER-003, PEER-005 |
| DS-C09 | Own-player card | CMP-READMODEL, CMP-INGEST | DATA-001–DATA-004, UI-002, UI-005 |
| DS-C10 | Enemy card | CMP-READMODEL, CMP-VISION | DATA-005, UI-002–UI-005, ALERT-006 |
| DS-C11 | Ally / peer card | CMP-READMODEL, CMP-PEER | UI-002–UI-004, PEER-002–PEER-005, PEER-008–PEER-009 |
| DS-C12 | Map and marker | CMP-READMODEL, CMP-VISION | DATA-004–DATA-005, VISION-001–VISION-003, UI-002, UI-005–UI-006 |
| DS-C13 | Inspector, skill, and item detail | CMP-READMODEL, CMP-ENTITLEMENT | DATA-003, DATA-006, UI-005–UI-006, PREMIUM-001–PREMIUM-003 |
| DS-C14 | Estimated timer | CMP-READMODEL, CMP-INGEST, CMP-ENTITLEMENT | UI-007, DATA-004, PREMIUM-001–PREMIUM-003 |
| DS-C15 | Event feed | CMP-READMODEL, CMP-MEDIA, CMP-ALERT | UI-002, MEDIA-001–MEDIA-003, ALERT-004–ALERT-006 |
| DS-C16 | Audio controls and PTT | CMP-VOICE, CMP-MEDIA, CMP-ALERT | VOICE-001–VOICE-007, MEDIA-005, QUALITY-006–QUALITY-007 |
| DS-C17 | Consent and peer join | CMP-PEER, CMP-ADVICE, CMP-VOICE, CMP-SOCIAL | PEER-001, PEER-007–PEER-009, ADVICE-002–ADVICE-004, VOICE-001, SOCIAL-001–SOCIAL-003 |
| DS-C18 | Risk, alert, and event-media presentation | CMP-ALERT, CMP-RISK, CMP-MEDIA | ALERT-001–ALERT-007, MEDIA-002–MEDIA-005 |
| DS-C19 | Voice-pack row and activation | CMP-MEDIA | MEDIA-004, MEDIA-006–MEDIA-008 |
| DS-C20 | Recorder and clip detail | CMP-RECORDER | OBS-001–OBS-005, VISION-004 |
| DS-C21 | Summary, local history, and forgetting | CMP-MEMORY | MEM-001–MEM-005, OBS-003, PEER-009 |
| DS-C22 | Settings and overlay editor | CMP-READMODEL, CMP-STARTUP | CORE-004–CORE-005, UI-001, UI-006, VOICE-002 |
| DS-C23 | Dialog, popover, and action feedback | CMP-READMODEL | UI-006; consuming action's requirement |
| DS-C24 | Companion presence | CMP-READMODEL, CMP-VOICE | UI-001, UI-002, VOICE-005–VOICE-007, MEDIA-005 |

## DS-C01 — Application shell and navigation

**Anatomy and geometry.** One authored 1420 × 760 stage; rail `(20,20,88,720)`; panel `(120,20,1280,720)`. The rail contains the original mark, seven named destination links in canonical order, and a bounded quiet status/footer area. It never expands horizontally. Main content is a labelled landmark with exactly one route heading. Overflow belongs inside the active panel's named scroll regions.

**Tokens and variants.** Canvas/panel/raised surface roles; primary/secondary text; accent for the active route plus a persistent shape/indicator. Active, inactive, hover, focus, and unavailable-destination states are independent. An unavailable Store destination remains navigable to its explanation; it does not become a dead unlabeled icon.

**Events.** `navigate(destination)` changes the control route only. Visiting a route does not start the mic, recording, peer room, preview, or account request. Route state retains the user's bounded list position when safe; source/session changes still invalidate protected content.

**Accessibility and source boundary.** Use `nav` with a name and links with `aria-current="page"`; keep Thai labels visible where possible and full labels accessible. No keyboard trap. Control routing cannot toggle or focus the independent native overlay. Optional service failure cannot gate the local routes.

## DS-C02 — Header and bounded page section

**Anatomy.** A 72 px panel-local header contains route title, optional brief contextual subtitle, and no more actions than its fixed width can support. Secondary actions move into a named overflow menu. A section consists of heading, description, bounded body, and optional footer action row.

**Tokens and states.** Heading-md for the route; heading-sm for sections; supporting text for metadata. A quiet source-status chip may sit next to a title, but it never replaces field-level provenance. Skeletons retain the same header and region bounds without displaying fabricated numeric values.

**Events and accessibility.** Header buttons follow DS-C03. A programmatically focused region heading uses temporary `tabindex="-1"`; it is not an extra tab stop. Bounded scrolling must reveal focused controls and their outline without scrolling the page or moving the map. A route title is not automatically announced on each data update.

## DS-C03 — Button, icon action, and link

**Anatomy.** Label plus optional leading icon; optional short keyboard hint aligned as supporting text. Default height 44 px with 16 px horizontal padding; compact height 36 px for in-field actions. Icon-only buttons retain a square target and an accessible action name.

**Variants and tokens.** Primary uses accent/on-accent. Secondary uses raised surface, primary text, and strong boundary where needed. Quiet uses readable text with a clear hover/focus treatment. Destructive uses danger text or a deliberate danger-filled confirmation. One primary action per local decision group. Text links use a persistent affordance such as underline where embedded in prose.

**States and events.** Rest, hover, pressed, focused, busy, disabled. `activate(intent)` fires once; Space keyup/Enter and pointer activation cannot double-submit. Busy keeps width and label context (`กำลังบันทึก…`) and prevents duplicate mutation. Native completion determines success. Disabled includes the nearby reason and exposes no executable handler; `aria-disabled` alone is insufficient unless code also suppresses activation.

**Boundary.** A primary-looking button does not imply paid/service access. Show unavailable information instead of an apparently working Buy/Subscribe/Upgrade control when no authority exists. Buttons never send telemetry about clicks by default.

## DS-C04 — Form field, checkbox, switch, and slider

**Anatomy.** Persistent label, control, helper text, validation text, and optional unit. Placeholders are examples only, never the sole label. Inputs use raised surface and a sufficiently identifiable border. Error text uses danger plus an icon, with the original input preserved.

**Variants.** Text/select fields; explicit checkbox for consent; immediate preference switch for already-understood reversible settings; output-volume slider with numeric percentage and mute control. A password or OBS credential is masked and never reflected into diagnostic copy. Hotkey capture is a named control with current binding, new candidate, cancel, and registration result.

**Events and states.** `edit(value)` is local draft state. `commit(value)` follows validation. `togglePreference(enabled)` changes only that preference. `captureBinding(candidate)` becomes effective only after native registration succeeds; failure retains the former binding and setting. A checkbox starts unchecked whenever fresh consent is required. Unknown device capability is unavailable, not an off setting.

**Accessibility.** Bind label, description, and error IDs; expose invalid state only after appropriate validation. Support keyboard slider increments and text output, not color alone. Checkbox/switch state appears in both semantics and a visible indicator. Consent controls wrap text and preserve Thai marks. Do not encode critical conditions only in tooltips.

## DS-C05 — Tabs and history filters

**Anatomy.** Fixed-height tab list, current-tab underline/shape, and one associated panel. Filters sit within the active tab and never masquerade as additional primary navigation. Live uses its four agreed subviews; Insights uses Summary and History only. History has All / Sessions / Clips filters.

**Tokens and states.** Supporting/label typography; current state uses accent text and a visible indicator. Hover and focus remain distinct from selected. Disabled subviews include a reason and do not vanish in a way that changes map geometry.

**Events and accessibility.** `selectTab(id)` changes only the active local panel. Arrow keys/Home/End move in the tab list. Use automatic activation only for immediately available local content; otherwise Enter/Space activates. Tabs use `tablist`, `tab`, `tabpanel`, `aria-selected`, and a managed roving tab stop. Keep focus on the selected tab until the user moves it.

**Boundary.** Switching right-side feed tabs or a fixture tier while the Live field is displayed cannot resize the map. Returning from another Live subview restores the same authored field geometry. Selecting Clips filters local history and does not start a recorder. Leaving an inspector subview does not delete native timers.

## DS-C06 — Status badge and provenance line

**Anatomy.** Status icon/shape, short readable state word, and optional source/age line. Status words remain meaningful in monochrome. Use `--kb-color-state-{unknown,stale,estimated,catalog,unavailable}` or the corresponding success/warning/danger/info semantic roles; never derive a meaning from accent color alone.

**Variants.** Fresh observation, stale, unknown, catalog-only, manual estimate, user-disabled, unavailable, and failed action. Source labels distinguish `ข้อมูลในเครื่อง` (local observation), `ภาพที่ยืนยันแล้ว` (admitted capture), `ข้อมูลจากเพื่อนร่วมทีม` (peer), and `ข้อมูลอ้างอิง` (catalog).

**Events and source boundary.** Optional `showProvenance(fieldKey)` reveals observation time, age, source revision/generation, and relevant validity reason. Native/admitted state owns freshness; polling does not refresh age. A heartbeat badge cannot grant fresh status to every field. At 2,001 ms peer vitals become stale/null according to their producer contract, even if the page is still open.

**Accessibility.** Include the meaning in the field's accessible description. Announce a meaningful action outcome or transition once; suppress per-tick age announcements. A tooltip may add technical provenance, but visible stale/estimated labels are mandatory where interpretation changes.

## DS-C07 — Empty, loading, unavailable, and error panel

**Anatomy.** Quiet functional icon, short heading, one explanatory sentence, and at most one primary recovery action plus a secondary settings/help link. Preserve the exact allocated region. No illustration may resemble a live minimap, populated inventory, or measured chart.

| State | Exact heading | Supporting behavior |
| --- | --- | --- |
| No match | `รอเข้าเกม` | `ข้อมูลที่ยืนยันได้จะแสดงเมื่อพร้อม` — no seeded roster. |
| No history | `ยังไม่มีประวัติในเครื่อง` | Explain that admitted local sessions will appear here. |
| Loading | `กำลังโหลด…` | Actual in-flight work only; bounded producer timeout remains authoritative. |
| Source missing | `ยังไม่มีข้อมูลที่ยืนยันได้` | Identify the missing source or admission condition. |
| Unsupported profile | `ยังไม่รองรับการตั้งค่านี้` | Explain the affected capability and applicable configuration. |
| Deferred service | `ยังไม่เปิดใช้ในรุ่นนี้` | No fake account/commerce operation. |
| Failed operation | `ดำเนินการไม่สำเร็จ` | Specific cause, retained state, and real recovery action. |
| Turned off | `ปิดโดยคุณ` | Link to the user's preference; do not call this a fault. |

**Events and accessibility.** `retry(operation)` retries the named operation only after current consent/generation validation; `openSettings(category)` is explicit navigation. Use a readable region, not an automatic modal. Do not focus an error that arrives while the game has focus. Show safe translated error descriptions; never expose a raw exception, credential, token, local transcript, or hidden peer payload.

## DS-C08 — Stat value and meter

**Anatomy.** Label, value, unit, optional known maximum, source state, and optional meter. Use primary text for value and supporting text for unit. The meter is redundant with text, not the only encoding.

**Variants.** Admitted value (including zero), known bounded percentage, unknown, stale local value where permitted, and withdrawn peer value. Unknown is an em dash with no filled bar. Admitted HP `0` may render a zero meter only when alive/dead semantics are separately supported. Do not compute a numerator or maximum from a percentage.

**Events and source boundary.** Read-only by default. `showProvenance` may open detail. Values, limits, readiness, and alive state are native-admitted facts, not renderer defaults. Peer expiry clears its live meter and values; it does not leave a dimmed but apparently current number.

**Accessibility.** A meter's accessible value exists only for an admitted numeric state. Unknown omits the numeric value and describes the absence. The containing value is not an ARIA live region. Updates must not change layout width or reorder cards.

## DS-C09 — Own-player card

**Anatomy.** In the 196 × 430 own region: identity/portrait, own-player indicator, admitted vital/stat groups, source state, and a visually separate companion area. Use bounded groups with 16 px inset; do not repeat self among the four ally slots. Keep the companion's activity separate from the player's HP or status.

**States and variants.** Neutral own identity placeholder; admitted own observation; stale; ownership unverified; known death; source unavailable. Show only the fields actually admitted. A missing portrait retains its size and readable name.

**Events.** `selectEntity(key)` opens the permitted base summary or eligible detail. Key includes session epoch, team, player slot, and hero identity/revision guards. Only deliberate activation selects; source refresh does not.

**Accessibility and boundary.** A clearly named selection control describes own hero and source condition. No nested interactive card. Selected-unit telemetry from another hero does not enter this card or an own-only skill view. Zero cooldown never changes its status to “ready” without readiness evidence.

## DS-C10 — Enemy card

**Anatomy.** Exactly five slots in the 616 × 72 enemy strip; 8 px gaps give 116.8 px per card. Each compact card shows an admitted portrait or stable placeholder, concise identity, seen/missing/unknown state, and admitted observation age. Preserve Thai line-height and truncate identity only with full accessible text.

**States.** Unknown slot, known identity/no current coverage, currently observed, valid missing observation, stale coverage, selected. Unknown age is `—`. “Missing” requires admitted prior observation and valid coverage; unknown coverage cannot generate an absence timer or reassurance.

**Events and boundary.** `selectEntity(key)` only if the identity/slot is known and admissible. A known marker and its card resolve to the same key. No ability/item/hidden-stat rows; no four-question-mark grids. Enemy positions come only from admitted visible capture; a paid tier cannot widen that source.

**Accessibility.** Card accessible name includes identity, observation status, and age when meaningful. Selection indicator uses a strong outline/shape. A click opens detail only; it never controls the game, starts a manual timer implicitly, or infers an unknown marker's identity.

## DS-C11 — Ally / peer card

**Anatomy.** Exactly four stable non-self slots in the 616 × 94 ally strip; 8 px gaps yield 148 px cards. Base anatomy is compact identity plus admitted state/age. Eligible fresh peer detail can add only the narrow vital contract within the same bounds.

**Variants.** Unknown ally, known base ally, consented/fresh peer, stale/withdrawn peer, selected. The peer badge identifies the source; it is not an account-friend or premium badge.

**Events and source boundary.** `selectEntity(key)` selects permitted data. `openCooperativeSharing` is a separate explicit link, never an autojoin. Peer fields are hero, level, HP/maxHP, mana percentage, alive and respawn status with age; no ability/item/position data. A fresh sample also needs correct room, match, team, membership, source, and presentation permission. At expiry/leave/withdrawal clear peer vitals immediately and reduce to admitted compact base content.

**Accessibility.** Do not announce every HP/respawn update. Announce explicit leave results once. Received data remains transient: this component offers no export-to-history, “ask AI about teammate,” persistent log, or cloud-send action.

## DS-C12 — Map and known marker

**Anatomy and tokens.** Panel-local `(228,168,616,430)` with an inset source/status strip that does not change the outer bounds. Canvas/raised surfaces, strong functional marker boundaries, and labelled no-image state. The neutral production state has no decorative live-looking markers.

**Variants.** Admitted live frame, stale frame where display is permitted, unavailable capture, GSI-only/Lite, calibration/profile unsupported, and explicitly labelled decorative non-live fallback. Never project live markers on a fallback or frame with mismatched geometry/session/source generation.

**Events.** `selectKnownMarker(entityKey)` equals selection from DS-C10/11; unknown markers have no invented identity-selection handler. If supported, `showCaptureStatus` explains the admitted frame and geometry. There is no game movement, drawing automation, or click-through game command from the control minimap.

**Geometry and accessibility.** Maintain ≤1 CSS px variation in each measured bound under frozen input and inspector/feed/tier changes. Map points use admitted source-region dimensions and the actual contained-image rectangle including letterbox offsets. Never extrapolate between observed points. Provide equivalent named card selection for dense known markers, meaningful region text, and noncolor marker identity. Do not claim target-size compliance merely because a point exists on a map.

## DS-C13 — Inspector, skill, and item detail

**Anatomy.** Permanent right region `(860,84,404,620)`, with stable tabs/heading, selected identity, close button, provenance, eligible detail groups, and bounded scroll. Neutral state says `เลือกฮีโร่เพื่อดูข้อมูลที่มี` and offers the base event feed. Opening detail does not turn this region into a modal or push the map.

**Variants.** Neutral; base summary; eligible full inspector; catalog reference; own-observed skill; known inventory entry; unknown field; stale field; invalidated selection. Base/Pro examples on the board carry a visible fixture label outside the stage; production cannot read fixture tier as authority.

**Skill contract.** Preview up to four admitted entries plus `+N` when needed; complete list has variable length. Keep catalog description separate from observed learned rank and readiness. Render cooldown, charge recharge, passive, unlearned, insufficient evidence, stale, and manual estimate distinctly. A zero cooldown alone never displays `พร้อมใช้`. Catalog-only data says `ข้อมูลอ้างอิง` and exposes patch/revision/coverage.

**Item contract.** Distinguish known empty, unknown, stack count, charges, cooldown, and inventory group. Preserve groups such as main inventory/backpack as supplied by the admitted contract. Hide unknown-only rows. Clicking an item or ability expands explanatory detail only.

**Events.** `selectEntity`, `openInlineDetail`, `closeInlineDetail`, `expandBackpack`, `closeInspector`. Changing hero resets inline detail, backpack expansion, and local scroll. Session/source/identity changes fence late responses. Capability downgrade clears protected detail and selection coherently without deleting native timers or changing consent; regain stays neutral.

**Accessibility.** Escape closes an inner edit, then inline detail, then inspector. Close restores available card focus without activating the control window over the game. Announce one meaningful selection result; do not trap focus or announce countdown ticks. Keep focus on a safe control/heading if the selected card disappears.

## DS-C14 — Estimated timer (deferred projection)

**Anatomy.** Native-owned timer label, bounded remaining game-time value, explicit estimated-state badge, and allowed action if the capability is actually implemented. Only reviewed enemy ULT/BKB/TP scope applies, at most 15 keys; arbitrary board timers are fixture-only.

**States and events.** Running estimate, paused/stale clock, estimated expiry, unavailable capability. `requestManualEstimate(reviewedKey)` and allowed timer-management intents require native validation. `คาดว่าพร้อม` persists after expiry; it never becomes observed-ready. Closing the inspector or changing selection does not delete the timer.

**Tokens and accessibility.** Use `--kb-color-state-estimated` (the blue/info role) plus an explicit estimate label, stable tabular numerals, and no pulsing countdown. A stale clock adds its separate stale warning. Native authoritative game time owns advancement; browser wall time cannot make a timer expire during stale/paused clock. No per-second screen-reader announcements. Production with absent authority displays unavailable information and no seemingly working timer control.

## DS-C15 — Local event feed

**Anatomy.** Bounded list inside the reserved right region, with tabs/filter, event label, game/source timestamp where known, concise context, and explicit provenance/partial state where applicable. Base access remains available. Events have stable IDs so new data does not replace a user's focused row.

**States.** Empty, loading existing local history, admitted event, source gap, failed retrieval. No match-start, first blood, kill, level, or respawn is seeded from a reconnect baseline. A quiet/unvoiced event can still appear if admitted; feed presence is not proof of audible playback.

**Events.** `filterEvents`, `openEventEvidence`, and `openRelatedLocalClip` where a real indexed clip exists. A “jump to latest” action appears if the user has scrolled away; do not continuously yank scroll/focus. If a feed item refers to selected detail, navigation is explicit.

**Accessibility and boundary.** Use a named list; announce a concise explicit-action result, not every event automatically. The event taxonomy and ordinary ranks come from the versioned native catalog. Respawn remains unvoiced. No received peer sample is persisted into this timeline.

## DS-C16 — Audio controls and local PTT

**Anatomy.** Output mute button, volume slider/value, current pack/audio readiness, PTT binding, microphone-consent state, and one lifecycle status. Separate mute from microphone permission. A waveform, if implemented, represents local capture only while actually listening and does not persist samples.

| Native lifecycle / condition | Exact Thai label | UI behavior |
| --- | --- | --- |
| Mic opt-in false | `ไมโครโฟนปิด` | PTT does not open a stream; offer a separate consent path. |
| Idle with consent | `กดค้างเพื่อพูด` | Show current binding; no background listening. |
| Preparing local model | `กำลังเตรียมโมเดลในเครื่อง` | Bounded preparation; never pretend to be listening. |
| Listening | `กำลังฟัง` | Show held-turn state and a stop/cancel affordance if supported. |
| Decoding / answering | `กำลังประมวลผลในเครื่อง` | No cloud-send affordance or persistent transcript. |
| Speaking | `กำลังตอบ` | Native audio owns completion and paired visual lifetime. |
| Preempted / cancelled | `หยุดคำตอบแล้ว` | Clear stale answer visuals; no automatic replay. |
| Missing model | `ยังไม่มีโมเดลเสียงที่พร้อมใช้งาน` | Explain local setup; no remote fallback. |
| Device/permission failure | `เปิดไมโครโฟนไม่ได้` | Specific recovery in settings; no stream indicator. |
| Open timeout | `เปิดไมโครโฟนไม่ทันเวลา` | Release native resources; allow an explicit fresh attempt. |
| Binding conflict | `ปุ่มลัดนี้ถูกใช้งานแล้ว` | Preserve the previous working binding. |

**Events.** `requestMicConsent`, `pttPress`, `pttRelease`, `cancelTurn`, `setMute`, `setVolume`, `requestBindingChange`. Native minimum hold is 250 ms, maximum turn 15 s, and open timeout 2 s; repeated held presses/unmatched releases cannot create work. One fenced turn at a time.

**Accessibility and source boundary.** Hold-to-talk must remain a genuine hold. Registration conflicts are visible and transactional. Voice audio, transcript, free-form question, interpretation, and answer stay local and transient. Critical/announcer preemption cancels superseded voice and visual tail. UI completion cannot claim the acoustic 2 s/quality gates; those need measured audio evidence.

## DS-C17 — Consent and cooperative join

**Anatomy.** Specific purpose heading, plain-language recipient/data explanation, unchecked consent control when fresh opt-in is required, explicit activation button, and visible cancel/leave path. Each consent has its own control and state. Account sign-in, provider choice, friendship, premium authority, and an invitation are not consent.

| Purpose | Exact v1 consent wording / action | Boundary |
| --- | --- | --- |
| Local microphone | `อนุญาตให้ใช้ไมโครโฟนเฉพาะตอนกดค้างเพื่อพูด เสียงและคำถามประมวลผลในเครื่อง` / `อนุญาตการใช้ไมโครโฟน` | Starts no stream until a subsequent valid hold. |
| Peer create/join | `ฉันยินยอมแชร์ข้อมูลฮีโร่ เลเวล HP สูงสุดและปัจจุบัน เปอร์เซ็นต์มานา สถานะมีชีวิตและเวลาเกิดใหม่กับสมาชิกในห้องและทีมเดียวกันของแมตช์นี้` / `ยินยอมและเข้าร่วมห้อง` or `ยินยอมและสร้างห้อง` | Fresh checkbox and explicit activation together; no saved join, autojoin, or view-only mode. |
| Optional cloud context | `อนุญาตให้ส่งเฉพาะบริบทเกมที่ระบุไปยังผู้ให้บริการที่เลือกเพื่อขอคำแนะนำ` / `อนุญาตบริบทเกมที่ระบุ` | Expanded visible field list is required before activation; no free-form voice or personal-memory send. |
| Social presence | `อนุญาตให้แสดงสถานะออนไลน์แก่ผู้รับที่ระบุ` | Must reflect the implemented recipient/presence contract; cannot grant peer game-data sharing. |

The cloud field list reflects the closed projection: coarse phase; own hero, level, kills/deaths/assists, net worth, gold, HP percentage and mana percentage; team score; and fixed non-player instructions. Unknown allowed fields stay unknown. Do not include enemy/peer data, items, exact clock, voice/transcript, memory, account IDs, or a serialized local context. The [cloud schema](../../contracts/cloud-context.schema.json) is a proposed contract; final native projection remains governed by ICD and ADVICE-003.

**Room presentation.** Show admitted membership as `สมาชิก {n}/5`, including the owner. Rooms last at most 4 hours; unsynced membership expires after 30 s under the producer contract. Display the producer's remaining/expiry state, not a browser timer that renews membership. Owner leave closes the room; code rotation invalidates future old-code joins without itself kicking existing members. A closed/expired room requires explicit fresh join.

**States and events.** Off, draft checked, joining/request pending, active, declined, revoked, membership expired, unavailable service, error. `joinRoom(code, freshConsentIntent)` may run only after validation; `leaveRoom` immediately clears local credentials/samples and fences old work. No replay of an old sample after invalid source. Leaving feedback: `ออกจากห้องแล้ว ข้อมูลที่รับไว้ในเครื่องถูกล้างแล้ว`; explain remote expiry separately without guaranteeing instant erasure.

**Accessibility and boundary.** Do not put long consent into a tiny tooltip or preselected “recommended” toggle. Checkboxes wrap across lines; dialog focus begins at the explanation/first sensible control, not the affirmative button. Tokens/credentials remain native; neither a room code nor a trusted origin appears in a log without the governing rules. A consent withdrawal invalidates pending work and prevents late responses from reviving state.

## DS-C18 — Risk, alert, and event-media presentation

**Anatomy.** Small risk indicator with heuristic label and coverage state; separate native-admitted alert banner; paired media slot controlled by native audio lifecycle. The control-window feed and passive overlay can represent the same event without creating independent detectors.

**Variants.** Informational heuristic, admitted danger, admitted revision/correction, missing notification, no coverage, critical-media not ready, and invalid/unmapped ordinary media. Use danger/warning/info roles with an icon and literal text. No coverage is neutral/unavailable, never a green “safe” result. A low score alone does not render a confirmed all-clear.

**Events and boundary.** `openAlertEvidence` is a deliberate control-window action. Critical admission is native and independent of renderer/network/model delay. Missing audio requires absence strictly longer than 5 s and native policy; the UI does not trigger an event merely because its displayed age ticks past 5. Ordinary unmapped media remains silent after rank selection. Critical media missing displays `เสียงเตือนสำคัญยังไม่พร้อม`; it does not synthesize a substitute.

**Media lifecycle and accessibility.** Active verified package revision is the only event audio/visual source. Stop, cancellation, and preemption remove both with the same generation guard. A non-live board banner is labelled as a specimen outside the product stage and does not autoplay audio. Native overlay has no focusable actions; a readable equivalent is available in the control feed. Color is supplemental to text; never flash a warning continuously.

## DS-C19 — Voice-pack inventory and activation

**Anatomy.** Local pack title, author/publisher metadata if verified, revision, installation state, integrity/trust result, active-state indicator, explicit preview, and activation action. Keep installation success separate from active selection. No game-media artwork is invented as a placeholder.

**States.** Not installed, validating, installed/verified, installed but activation deferred, active, invalid package, publisher untrusted, entitlement unavailable if applicable, and preview playing. `ติดตั้งแล้ว — รอเปิดใช้` is distinct from `กำลังใช้งาน`.

**Events.** `chooseLocalPackage`, `installPackage`, `requestActivation`, `previewVerifiedAsset`, `stopPreview`. Native receipts govern each transition. A deferred activation leaves the old active revision usable. Preview is an explicit user request, labelled `ตัวอย่างเสียง`; it is not injected into the live event timeline or used to claim an event occurred.

**Accessibility and source boundary.** Preview/stop are named buttons with visible playback state. No automatic preview on hover, focus, or route visit. Package integrity, trusted publisher, purchase authority, and activation are separately represented. Invalid install cannot mutate the active package. A pack's self-declared key or local hash validity never creates trusted/purchased status.

## DS-C20 — Recorder controls and local clip detail

**Anatomy.** Overview recorder card with status, explicit start/stop action, local output summary, and configuration link. Settings → System owns OBS configuration. Insights → History → Clips owns the indexed clip list/detail. These are views of one native recorder lifecycle.

| State | Exact Thai label | Allowed presentation |
| --- | --- | --- |
| Stopped | `ยังไม่ได้เริ่มบันทึก` | `เริ่มบันทึกในเครื่อง` if the implementation is ready. |
| Connecting | `กำลังเชื่อมต่อ OBS` | Show bounded pending state; no recording indicator yet. |
| Recording | `กำลังบันทึกในเครื่อง` | Persistent readable indicator and `หยุดบันทึก`. |
| Saving clip | `กำลังบันทึกคลิป` | Pending count from native queue; no completed thumbnail/file link yet. |
| Saved | `บันทึกคลิปแล้ว` | Real local file/evidence metadata and `ดูคลิป`. |
| Partial | `คลิปไม่ครบช่วงที่ขอ` | Actual interval plus reason beside requested interval. |
| OBS unavailable | `OBS ยังไม่พร้อมใช้งาน` | Local alerts remain independent; genuine setup/retry path. |
| Disk pressure | `พื้นที่จัดเก็บไม่เพียงพอ` | Explain stopped/bounded writes; no automatic destructive cleanup. |

**Events.** `startLocalRecording`, `stopLocalRecording`, `openRecorderSettings`, `openClip`, `openLocalFile`. Starting is explicit and does not enable mic, cloud upload, streaming, or replace the critical vision source. Native authentication uses loopback WebSocket v5; no credential is echoed into UI diagnostics.

**Coverage and accessibility.** Show requested `ก่อนเหตุการณ์ 30 วินาที / หลังเหตุการณ์ 15 วินาที`, actual start/end, and partial/trim status. A whole-buffer save is not exact-trim proof. Highlight eligibility consumes admitted events before audio suppression; an unvoiced event can still have a clip. Announce explicit saved/failed results once; pending queue updates are not incessant announcements. Browser interactions cannot certify real OBS, disk, timestamp, or resource behavior.

## DS-C21 — Summary, local history, and forgetting

**Anatomy.** Insights has Summary and History tabs. Summary shows at most three evidence-linked points per completed session, each with timestamp/source. History is a bounded list with filters, empty/partial states, local evidence links, and clip detail through DS-C20. Use simple evidence summaries, not a fabricated analytics dashboard.

**Variants.** No records, partial session, complete supported session, unknown outcome, unavailable snapshot, and memory-cleared state. A failed snapshot rebuild retains the prior valid snapshot or explicit unavailable state. Unknown win/loss/death cause is never supplied by a model narrative.

**Events.** `openEvidence`, `filterHistory`, `requestForgetMemory`, and a separately scoped history deletion action only if implemented/authorized. Forget confirmation title: `ล้างความจำที่สรุปไว้?`; description: `ล้างข้อมูลสรุปที่ระบบจดจำ โดยเก็บประวัติและคลิปในเครื่องไว้`; primary action: `ล้างความจำ`; cancel: `ยกเลิก`.

**Accessibility and boundary.** Focus initially favors the explanation/cancel for destructive confirmation; completion comes from the native durable cutoff result, not an optimistic empty list. Forget preserves separate raw history unless explicitly deleted. No automatic retention schedule or invented “delete everything” default. Peer samples and voice/transcripts never become history, memory, or summary evidence. Partial sessions remain labelled until the finalized completion contract can establish completeness.

## DS-C22 — Settings and overlay editor

**Anatomy.** Bounded category navigation for General, Overlay, Audio and alerts, AI, Modules and CV, Privacy, System; within a category, rows pair label/description/control/status. Overlay settings include per-module enabled, position, scale, opacity, saved profiles, and a clearly labelled editor preview.

**States and variants.** Saved, draft, applying, failed, user-disabled, capability-unavailable, unsupported/unverified profile. A missing CV observation does not silently disable its preference. A preview is labelled `ตัวอย่างการจัดวาง — ไม่ใช่ภาพสด`; no decorative marker becomes a live fact.

**Events.** `changeSetting`, `applySettings`, `cancelDraft`, `saveOverlayProfile`, `loadOverlayProfile`, `remapShortcut`, `openIntegrationSetup`. Preserve customized values during ordinary migrations. Do not reset unrelated game configuration; native setup writes only its owned file and verified selected loopback port. Failed apply preserves the previously valid setting where the contract permits.

**Accessibility and boundary.** Category changes do not submit unsaved forms without a clear policy. Keyboard users can manipulate position/scale via labelled numeric controls in addition to pointer editing. A rendering preview cannot establish protected game-HUD clearance, always-on-top behavior, or click-through. Unsupported mode/capture geometry is identified without blocking independent valid local functions. System resource metrics show unavailable when unmeasured; no fabricated CPU/RAM gauge.

## DS-C23 — Dialog, popover, and feedback

**Anatomy.** Modal dialog: labelled title, concise description, bounded body, explicit cancel/close and primary action. Popover: contextual content anchored to its trigger, no unnecessary modal trap. Action feedback: brief success/failure near its action, with a persistent details path when needed.

**Events and states.** `open`, `confirm`, `cancel`, `close`. Escape closes the innermost editable operation before its containing dialog/popover. A modal restores focus to its trigger when available; fallback is a meaningful local heading. Do not display a modal or toast for ordinary source refresh, Pro downgrade, or peer freshness expiry.

**Tokens and accessibility.** Raised surface, strong boundary, primary text, focus token; bounded backdrop applies only to the control window. Native dialog semantics/name/description are required. Focus is contained for a real modal task and free for the inspector. Success feedback does not steal focus or play audio. A critical failure has visible text and a sensible recovery path; it does not auto-dismiss before the user can read it.

**Boundary.** Dialog confirmation invokes a validated intent; it is not itself evidence of native completion. A destructive label must state the affected data, and memory forgetting must not implicitly authorize history deletion. Error detail follows the safe error catalog, not raw exceptions.

## DS-C24 — Companion presence

**Anatomy.** Original Kohbor mark/companion representation and one short activity label. In the live field it is visually separate from own-player stats; in the passive overlay it occupies the inherited top-right vicinity pending measured safe-area placement.

**Variants.** Idle, local preparation, listening, local processing, speaking, paused/unavailable. The graphic remains calm; no perpetual bobbing or excited motion tied to an unknown game result. Use the same Thai status language as DS-C16.

**Events and boundary.** Decorative presence has no implicit action. A control-window `openAudioControls` affordance is a separate labelled button. The native voice/audio lifecycle owns activity; a mascot animation cannot claim a model is ready or speech is playing. Missing event media never causes the companion to synthesize an alert. The passive overlay representation is not focusable and cannot intercept input.

**Accessibility.** Decorative graphics are hidden from the accessibility tree when adjacent text already supplies the meaning. Announce meaningful action transitions only. Respect reduced motion and avoid animation that changes the own-card or map layout.

## Composition rules and implementation handoff

1. Compose Overview from DS-C02, C06–C08, C16, C20, and C24. Empty source values remain empty even if a model or account is connected.
2. Compose Live from the exact C09–C13 regions, C15, and scoped C17. Keep all five enemy/four non-self ally slots and the fixed right region in every state.
3. Compose Voice Packs from C19 plus C07 for installation/verification errors. Native package receipts own all displayed trust/activation claims.
4. Compose Insights from C05, C20, and C21. Its fixture may demonstrate local evidence, but production starts with no invented records.
5. Compose Store/Account unavailable views from C07 and real supported local navigation; no speculative price, balance, transaction, recovery, or upgrade control.
6. Compose Settings from C04 and C22. Local mic, cloud context, peer sharing, presence, and recording remain separate decisions.

Use [DESIGN-ACCEPTANCE](DESIGN-ACCEPTANCE.md) before claiming a component implemented or accepted. Design documentation and a browser specimen are not substitutes for native permission enforcement, source admission, or acoustic/resource acceptance.
