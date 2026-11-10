---
title: "Kohbor Design System — Nightglass"
doc_id: "DESIGN-SYSTEM"
status: "design_elaboration"
version: "1.0.0"
spec_baseline: "0.3.1 / DEC-BASELINE"
authored: "2026-11-11"
timezone: "Asia/Bangkok"
decision_scope: "DEC-VISUALS; delegated presentation choices"
approval_status: "not_approved"
native_acceptance: "NOT_RUN"
---

# Kohbor Design System 1.0.0

**Kohbor / เกาะเบาะ is a quiet local gaming companion.** The presentation direction, **Nightglass**, uses smoked graphite surfaces, restrained mint actions, legible Thai and English, and explicit evidence states. Its first task is to help the player distinguish what is known, what changed, and what requires an action.

This document makes the delegated visual choices implementable against specification **0.3.1**. Design-system version **1.0.0** is an artifact version; it does not change the PRD/SRS baseline, claim owner approval, certify Windows behavior, or close the native-measurement portion of DEC-VISUALS. The [PRD](../product/PRD.md), [SRS](../requirements/SRS.md), [UX-SPEC](UX-SPEC.md), and [Document Control](../governance/DOCUMENT-CONTROL.md) remain the authority for scope, permissions, behavior, and evidence.

## 1. Deliverables and ownership

| Artifact | Implementation purpose |
| --- | --- |
| [Design board](../../design/board/kohbor-design-board.html) | Self-contained visual reference and deterministic interaction fixture. Every simulated source or capability remains labelled as a fixture. |
| [Component catalog](COMPONENT-CATALOG.md) | Anatomy, tokens, state, events, focus, source boundaries, and requirement mappings. |
| [Design acceptance](DESIGN-ACCEPTANCE.md) | Repeatable design checks, evidence classes, and the unresolved native gates. |
| [Primitive tokens](../../design/tokens/source/primitives.tokens.json) | Raw palette, scales, dimensions, and primitive values. |
| [Semantic tokens](../../design/tokens/source/semantic.dark.tokens.json) | Meaningful surface, text, status, interaction, and layout roles. |
| [Component tokens](../../design/tokens/source/components.tokens.json) | Component-specific decisions that reference shared roles. |
| [Generated CSS](../../design/tokens/tokens.css) | Consumable `--kb-*` properties; regenerate from the token sources. |
| [Merged tokens](../../design/tokens/kohbor.tokens.json) and [resolved tokens](../../design/tokens/tokens.resolved.json) | Alias-preserving and resolved handoff formats. |
| [Original mark](../../design/assets/kohbor-mark.svg) and [wordmark](../../design/assets/kohbor-wordmark.svg) | Product-owned vector identity; no inherited game/product artwork. |

The token sources own values. The component catalog owns reusable behavior and visual contracts. The board demonstrates those contracts; it cannot grant production capabilities or relax a requirement. Conceptual event names in these documents describe UI intent, not a newly frozen IPC protocol. DEC-INTERFACES still governs wire names, validation, authority, and native transactions.

## 2. Principles that affect implementation

1. **Map first.** The minimap is the largest live-field region. Enemy visibility and missing information follow. Own status, the companion, and optional detail remain subordinate.
2. **Unknown stays visible.** An unavailable observation renders an em dash or a precise no-data statement. It never renders `0`, a full bar, a green check, or an invented portrait.
3. **One explicit action, one understandable effect.** Selecting a hero opens detail; it does not cast, buy, join a room, grant consent, start a recording, or send context.
4. **Stable geometry protects attention.** A refresh, tier transition, stale source, long name, or open inspector does not move the map. Overflow belongs inside bounded regions.
5. **A quiet companion interrupts only with authority.** Visual polish never creates audio, bypasses native priority, substitutes missing event assets, or promotes a heuristic into a confirmed threat.
6. **Local functionality remains approachable.** Sign-in, optional services, and deferred commercial capabilities cannot become a gate on available local gameplay functions.

These principles elaborate UI-001–UI-006, DATA-001–DATA-006, ALERT-001–ALERT-007, CORE-001, and PREMIUM-003; they add no data source or permission.

## 3. Identity and visual language

### 3.1 Name and mark

Use **Kohbor** in English and **เกาะเบาะ** in Thai. “Nightglass” names this design direction only. Do not rename the application, invent an account-service domain, or infer technical identifiers from the mark; [product-identity.json](../../contracts/product-identity.json) and DEC-NAME govern those identifiers.

The SVG mark is the canonical small identity asset. Prefer the mark plus live text for accessible application headings. Use the wordmark for cover/reference treatments where its shape is the subject. Keep a clear space of at least one quarter of the mark's rendered width on each side. Author the mark at 24, 32, or 48 px for chrome and at larger sizes for a board cover; inspect rasterization at the actual native scale before certifying a small use.

Product identity is not a status signal. A mint logo does not mean capture is healthy, the microphone is open, the recorder is running, or a model is ready. Companion presence gets a separate, readable state label. Missing assets use stable neutral shapes and readable identity text; they do not pull an arbitrary online image.

### 3.2 Surfaces and color

| Role / CSS token | Value | Use |
| --- | --- | --- |
| `--kb-color-surface-canvas` | `#0B1014` | Stage, deepest background. |
| `--kb-color-surface-panel` | `#121A21` | Main panel, stable containers. |
| `--kb-color-surface-raised` | `#1A252D` | Cards, inputs, inspector groups. |
| `--kb-color-text-primary` | `#EEF3F0` | Headings, important values, primary labels. |
| `--kb-color-text-secondary` | `#ACBBC3` | Supporting descriptions and source labels. |
| `--kb-color-text-muted` | `#8C9FA9` | Quiet metadata that still needs to be read. |
| `--kb-color-accent` | `#8DE0CA` | Primary action, current route, selected control. |
| `--kb-color-on-accent` | `#0B201A` | Text and icons on filled mint controls. |
| `--kb-color-success` | `#86D7A5` | Confirmed successful action or admitted healthy capability. |
| `--kb-color-warning` | `#F3C578` | Stale, partial, or attention-needed condition with text. |
| `--kb-color-danger` | `#F58F8B` | Action failure, destructive action, or native-admitted critical condition. |
| `--kb-color-info` | `#9DBFF5` | Reference information, labelled estimates, and neutral explanation. |
| `--kb-color-border-default` | `#344650` | Decorative separation between already identifiable surfaces. |
| `--kb-color-border-strong` | `#637B89` | Interactive boundaries and meaningful outlines where needed. |
| `--kb-color-focus` | `#BDEFFF` | Keyboard focus, paired with a dark separation gap. |

Use status colors as text/icon accents or as their defined low-chroma surface pairs, with a status word and recognizable icon. Avoid full-screen red, green map washes, constant glowing edges, gradients behind small text, and repeated accent-filled cards. A border in `border-default` is decorative: it is not sufficient by itself to identify an input or selected state. The strong border is an approved meaningful boundary on canvas/panel/raised backgrounds; it is not an approved sole boundary on the hover background. Use an adequately contrasting selection/focus cue there. Interactive components use a strong outline, a readable label/icon, and the focus treatment appropriate to their role.

Do not lower an entire card's opacity to communicate stale or disabled. It dims the explanation as well as the data. Keep explanation text readable; apply the dedicated state token to the status indicator. Opacity settings for the passive overlay must be measured against actual game imagery before acceptance.

### 3.3 Typography and Thai behavior

Use `--kb-font-family-ui`: **Noto Sans Thai, Noto Sans, Leelawadee UI, Tahoma, system-ui, sans-serif**. The board bundles unmodified regular/bold Thai and Latin Noto font files from [the font asset directory](../../design/assets/fonts/README.md), with [the included OFL license](../../design/assets/fonts/OFL-Noto.txt); its self-contained build embeds them and makes no runtime font request. This demonstrates offline board typography, not native font acceptance. A production bundle must carry the licensed fonts or validate its fallback on the supported Windows matrix. Inspect actual matching of intermediate weights and native fallback; token weight 600 alone is not proof of a distinct 600 font file.

| Role | Authored size | Line height | Typical weight | Usage |
| --- | --- | --- | --- | --- |
| Supporting / label | 14 px | 1.6 | 400 / 500 | Metadata, card status, navigation, field labels. |
| Body | 16 px | 1.6 | 400 | Descriptions, consent, error recovery. |
| Small heading | 20 px | 1.3 | 600 | Card group, inspector title. |
| Medium heading | 24 px | 1.3 | 600 | Route title. |
| Large heading | 32 px | 1.3 | 600 | Major dashboard number or short overview heading. |
| Display | 48 px | 1.2 | 600 | Design-board cover; avoid routine live UI. |

These are authored values, not measured minimums after stage scaling. The 14 px supporting size is never reduced to fit a longer string. Preserve Thai combining marks, top vowels, tone marks, and descenders; avoid fixed text boxes tighter than the token line height. Do not apply Latin-style uppercase or tracking to Thai. Labels use sentence case in English.

Use numerals `0–9` for game values in both interface languages, with locale-aware grouping and decimals where appropriate. Durations use units (`8 วินาที`, `8 s`); unknown durations use `—`. Game time, elapsed observation age, and local history timestamps are separately labelled. Use tabular numeric glyphs when the selected font supports them, and allocate stable width to frequently updated values. Do not announce numeric ticks to assistive technology.

Long labels wrap in prose and consent. Compact hero names use a single-line ellipsis with the full name available through the accessible name and focusable detail. Critical uncertainty, a destructive consequence, or a consent explanation must never be tooltip-only.

### 3.4 Space, shape, iconography, and elevation

Use the 4 px spacing rhythm through `--kb-space-1` to `--kb-space-12`. Prefer 8 px between tightly related controls, 12 px between compact groups, 16 px card insets/gutters, and 24 px between sections. Exact live-field dimensions in §5 take precedence over generic spacing guidance.

Use the radius scale consistently: compact controls/small badges, cards, and major panels take progressively larger roles. A pill is reserved for a short state label or segmented control. Do not round every rectangle into a pill. Elevation is primarily surface tone plus border; use a restrained shadow only for popovers and dialogs. The permanent inspector is a panel region, not a floating modal.

Icons use a consistent 20 px authored box and approximately 2 px stroke, with 16 px only for noninteractive inline metadata. Icon-only actions retain the 44 px default target or documented 36 px compact target and a meaningful accessible name. Use familiar functional shapes for home, map, sound, history, account, settings, close, and warning. Do not use emoji as production status icons or borrow source-product artwork.

### 3.5 Motion

Use instant state replacement for source invalidation and consent/capability withdrawal. Fast/standard token durations may animate hover, selected-tab emphasis, and a single opacity transition after a user action. No animation may delay removing stale peer vitals, clearing late output, restoring base presentation, or obeying native audio cancellation.

The map and reserved right region do not animate their bounds. Live markers do not interpolate a speculative path between observations. Loading uses a quiet text/spinner state only while genuine work is pending; there is no indefinite fake progress. Under reduced motion, remove nonessential travel, pulse, scale, and shimmer; retain immediate readable state changes. A design-board preview must never autoplay sound.

## 4. Information architecture and route copy

There are **seven main destinations**. English route names are functional labels, not a mandate for application URLs. The Thai labels below are the v1 copy contract. A retained destination can be unavailable or deferred; its presence does not promise a service.

| Main destination | Thai label | Bounded subviews / content | Scope and honest initial state |
| --- | --- | --- | --- |
| Overview | ภาพรวม | Local readiness, observed match summary, audio/PTT status, companion activity, recording controls. | MVP projection. Start with `รอเข้าเกม`; source-dependent values remain `—`. |
| Live | สนามสด | **Live field / สนาม**, **Build / บิลด์**, **CV heroes / ฮีโร่ที่ตรวจจับ**, **Cooperative sharing / แชร์กับทีม**. | Base field is shared by every tier. Peer sharing is an extension. Build shows admitted catalog/local capability or a clear unavailable state. |
| Voice Packs | แพ็กเสียง | Installed inventory, verification results, explicit preview, activation status. | Local verified import/activation are distinct from optional catalog/purchase services. |
| Store | ร้านค้า | Catalog, wallet, inventory, transaction-history information only when the corresponding service exists. | Deferred service: `ร้านค้ายังไม่พร้อมใช้งานในรุ่นนี้`. Do not draw a working checkout, fake price, balance, or subscription. |
| Insights | สรุปการเล่น | Exactly two tabs: **Summary / สรุป** and **History / ประวัติ**. | MVP local evidence. No prefilled win rate, invented outcome, or deep-analysis claim. |
| Account | บัญชี | Optional sign-in, linked identity, supported account settings. | `ใช้งานฟังก์ชันในเครื่องได้โดยไม่ต้องเข้าสู่ระบบ`. Unsupported account-security functions show unavailable. |
| Settings | ตั้งค่า | General, Overlay, Audio and alerts, AI, Modules and CV, Privacy, System. | Each category explains current availability; deferred capabilities cannot look enabled. |

### Local clips placement — delegated design choice DS-DEC-01

**Insights → History → Clips filter** is the single local clip browser. The History filter choices are `ทั้งหมด` (All), `เซสชัน` (Sessions), and `คลิป` (Clips); they filter one history list, not create a third Insights tab or an eighth main destination. Opening a clip reveals a bounded detail panel with event time, requested interval, actual coverage, partial reason, and local file action. Back returns to the same filter, list position, and focused row.

Overview holds the explicit start/stop recording control and current recorder status. Settings → System → Local recording holds OBS connection/output configuration, with a link from the Overview recorder card. A saved-clip acknowledgement offers `ดูคลิป`, which navigates to History with the Clips filter and selects that local entry. These routes never start recording by visiting them.

## 5. Fixed desktop geometry

### 5.1 Coordinate systems

The **authored stage is 1420 × 760**. The **content panel is 1280 × 720**, positioned at stage `(120,20)`. The navigation rail is at `(20,20)` with size `88 × 720`. Panel coordinates below are local to that content panel; do not mix them with stage or source-capture pixels.

| Region | x | y | Width | Height | Contract |
| --- | ---: | ---: | ---: | ---: | --- |
| Stage | 0 | 0 | 1420 | 760 | Authored canvas. |
| Navigation, stage coordinates | 20 | 20 | 88 | 720 | Seven destinations; no expanded rail changing panel width. |
| Main panel, stage coordinates | 120 | 20 | 1280 | 720 | All control routes fit this boundary. |
| Header, panel-local | 0 | 0 | 1280 | 72 | Route title, bounded route actions/status. |
| Own hero / companion, panel-local | 16 | 168 | 196 | 430 | Own observation and visually separate companion presence. |
| Enemy strip, panel-local | 228 | 84 | 616 | 72 | Five stable slots; 8 px gaps yield 116.8 px per slot. |
| Map, panel-local | 228 | 168 | 616 | 430 | Largest live-field region; immutable bounds. |
| Ally strip, panel-local | 228 | 610 | 616 | 94 | Four stable slots, excluding self; 8 px gaps yield 148 px per slot. |
| Summary / inspector / feed, panel-local | 860 | 84 | 404 | 620 | Always reserved; neutral initially, internally scrollable. |

The map occupies 264,880 authored square pixels, more than the 250,480 square pixels of the right region. The decimal enemy width is intentional: use a five-column fractional grid rather than rounding each width independently. Content panels, not the whole page, may scroll. The inspector, history, pack list, and settings forms must provide bounded vertical scrolling; tab bars and their labels remain visible.

### 5.2 Uniform stage fitting

For an **available inner content area** of `W × H`, calculate:

```text
scale = min(W / 1420, H / 760)
originX = (W - 1420 × scale) / 2
originY = (H - 760 × scale) / 2
```

Transform the complete stage uniformly. Never squeeze individual fonts or independently resize the map, rail, or right region. Stage fitting is not a responsive/mobile redesign. Native window chrome and safe margins must be measured before choosing the actual available area; browser viewport dimensions cannot stand in for native client bounds.

| Illustrative available content box | Uniform scale | 14 px supporting text after scale | 36 px compact target after scale |
| --- | ---: | ---: | ---: |
| 1280 × 720 | 0.9014 | 12.62 px | 32.45 px |
| 1440 × 900 | 1.0141 | 14.20 px | 36.51 px |
| 1672 × 941 | 1.1775 | 16.48 px | 42.39 px |

The 1280 × 720 reference is therefore a **readability risk to measure**, not proof that 12.62 px Thai text is acceptable and not permission to declare a new minimum window. At this scale a 44 px authored default target becomes approximately 39.66 px. The minimum readable text, minimum native interactive target, DPI behavior, and safe inner area remain DEC-VISUALS work. If those measurements fail, record and resolve the baseline/design conflict; do not quietly remove the reference size or adopt a new layout.

### 5.3 Map invariance and projection

With frozen input and viewport, opening, changing, or closing selection; expanding skill/item detail; switching right-side tabs; and changing fixture tier may alter the map's measured x, y, width, and height by **no more than 1 CSS px each**. Compare `getBoundingClientRect()` in the same coordinate space. Pixel tolerance is not permission to animate the map or progressively drift it.

Draw live markers only over a capture with matching session, frame, source generation, admitted source-region dimensions, and calibrated geometry. Preserve the source aspect ratio inside the map region; letterboxing cannot become an unaccounted coordinate offset. If the image is a decorative fallback, label it `ภาพประกอบ — ไม่ใช่ข้อมูลสด` and remove live markers. Missing capture produces `ยังไม่มีภาพแผนที่ที่ยืนยันได้`; GSI-only/Lite mode says which CV-dependent information is unavailable. No coverage must never become a “safe” state.

## 6. Data-state and provenance grammar

### 6.1 State matrix

State is a field-level contract; a healthy application can contain a stale map, an unavailable recorder, and valid own-player observations at the same time.

| State | Exact Thai / English short copy | Presentation | Action rule |
| --- | --- | --- | --- |
| Empty, no active match | `รอเข้าเกม` / Waiting for a match | Neutral outline, no seeded roster or selected hero. | Offer integration/settings only if relevant. |
| Empty, no stored content | `ยังไม่มีข้อมูล` / No data yet | Short explanation of what will create content. | One genuine available next action; otherwise none. |
| Loading | `กำลังโหลด…` / Loading… | Bounded progress state; preserve region geometry. | Cancel where supported; do not invite duplicate requests. |
| Live / fresh | `ข้อมูลล่าสุด` / Current observation | Value plus meaningful source/age. | “Ready” requires separate readiness evidence. |
| Stale | `ข้อมูลเก่า` / Stale data | Warning icon, age, no affirmative safe/ready assertion. | Existing values only where the producer permits; peer vitals clear. |
| Unavailable | `ยังไม่พร้อมใช้งาน` / Unavailable | Explain missing capability/source and recovery. | Action appears only if recovery is implemented. |
| Error | `ดำเนินการไม่สำเร็จ` / Action failed | Specific failure and preserved prior state. | Retry only an allowed operation, never consent bypass. |
| Disabled by user | `ปิดโดยคุณ` / Turned off by you | Neutral off toggle plus settings context. | Re-enable only through the relevant explicit control. |
| Unknown field | `—` / Unknown | No filled meter or substituted numeric value. | Details explain missing evidence if useful. |
| Manual estimate | `คาดว่าพร้อม` / Estimated ready | Distinct estimated badge even after expiry. | No conversion to observed readiness. |
| Catalog-only | `ข้อมูลอ้างอิง` / Reference data | Patch/source revision and coverage label. | Does not assert learned rank, cooldown, or castability. |
| Deferred capability | `ยังไม่เปิดใช้ในรุ่นนี้` / Not available in this build | Neutral explanation; no fake working controls. | No purchase/upgrade action without real service authority. |

“Disabled” is an interaction property, not a synonym for “error” or “unknown.” A disabled action includes an adjacent reason. A user-disabled overlay module retains its configuration; a producer failure does not switch its preference off.

### 6.2 Observation facts

Every dynamic view model retains the field's value, availability, source, observed age, source generation, and revision. Where uncertainty changes interpretation, show it beside the value. A detail panel may expose more provenance, but it cannot be the only place that explains a stale timer or partial clip.

Own-source and peer freshness follow their producer contracts, including the **2,000 ms permitted-fresh / 2,001 ms stale** boundary. UI polling, a heartbeat, animation, or a rerender never creates a new observation timestamp. A human-readable “observed at” timestamp is not an independent freshness clock.

An admitted zero renders `0` with units. Unknown HP renders `—`; it does not imply alive or dead. A mana percentage cannot supply an invented current/max mana pair. Known empty inventory is different from unknown inventory. Parsed selected-unit skills remain unavailable until verified ownership admission succeeds. Enemy cards carry admitted visibility/position facts, never hidden vitals, skills, items, or extrapolated movement.

Received peer data is limited to the admitted hero/level/HP/maxHP/mana percentage/alive/respawn contract. It supplies neither skills/items nor positions; it never enters memory, history, advice, AI context, logs, or cloud projection. When peer data is withdrawn or becomes stale, clear those vitals and retain only the admitted compact base identity/state. See DATA-001–DATA-006 and PEER-002, PEER-003, PEER-005, PEER-008, PEER-009.

## 7. Interaction, focus, and accessibility

### 7.1 Targets and focus

Author ordinary actions at a minimum **44 × 44 px** target and compact in-field controls at **36 × 36 px**. Compact use is restricted to bounded high-density regions; it is not a global density toggle that shrinks every control. Keep at least 8 px between independent compact controls when space permits, and do not overlap invisible hit areas. Full-card selection may provide a larger target than its visible icon. Native scaled acceptance remains open as described in §5.

Use a **2 px focus outline** in `--kb-color-focus`, offset **3 px**, with enough inset space that bounded scrolling containers do not clip it. The indicator remains visible while the control is focused. Selected, hovered, pressed, and focused are independent states. Forced-color mode uses system colors and preserves outlines; do not disable the operating system's focus support.

The external accessibility references are design targets, not a claim of overall WCAG conformance. Text targets at least **4.5:1** contrast for normal text, and meaningful interactive/graphical cues target at least **3:1** against adjacent surfaces. W3C's target-size minimum is **24 × 24 CSS px**, subject to stated exceptions; this design's 44/36 authored targets are product choices, not a fabricated WCAG AA requirement. See the primary references in §11.

### 7.2 Keyboard and assistive technology

Tab follows visual task order: navigation, route header actions, route tabs, then the active content's explicit controls. Use native buttons, links, inputs, and correctly labelled tabs rather than clickable generic containers. Arrow keys move within a tab list; Enter/Space activates when manual activation is required for expensive content. Do not request data merely because a tab receives keyboard focus.

Selection starts neutral. A user activating a hero card or known marker chooses the same stable entity key. A refresh cannot open the inspector, move focus, or invent an identity for an unknown marker. On explicit selection, announce a concise result such as `เปิดรายละเอียด {hero}`; keep the inspector nonmodal. The user can reach its heading/actions through the next logical control path without being trapped.

Escape closes the innermost editable/expanded operation first, then inline skill/item detail, then the inspector. Closing returns focus to the selected card when it still exists. If session/source/identity/capability changes invalidate it, recover focus to an available local card or region heading only when focus is already inside the control window. Never activate the control window over the game.

Use polite status announcements for explicit action results and important availability transitions. Do not announce every countdown, age, health update, marker movement, or audio waveform. Keep an event timeline available for deliberate review. Dialogs are reserved for real modal tasks, including explicit destructive confirmation and scoped consent, with labelled title/description, focus containment, and restoration on exit.

| Shortcut | Scope | Meaning |
| --- | --- | --- |
| Ctrl+Alt+S | Required global overlay action | Toggle gameplay overlay visibility. |
| Ctrl+Alt+V, held | Required global PTT after consent | Local push-to-talk; user-remappable with conflict detection. |
| Alt+Up / Alt+Down | Control audio command; global registration only where required | Adjust volume by ten percentage points. |
| Alt+M | Mute command | Toggle mute; never silently repurpose for PTT. |
| Ctrl+K | Control window | Open command palette. |
| Ctrl+/ or ? | Control window | Open shortcut help; do not capture text input unexpectedly. |
| Enter / Space | Focused control | Activate the current card, known marker, or action. |
| Escape | Current local interaction | Close inner detail before parent surface. |

Browser fixture key handlers can demonstrate order; they cannot prove native global-hook registration or noninterference with Dota.

## 8. Audio, consent, recorder, and capability presentation

### Audio and microphone

Microphone consent begins off. The microphone opens only for a user-held PTT turn after that consent. The UI separates `ไมโครโฟนปิด`, `กำลังเตรียม`, `กำลังฟัง`, `กำลังประมวลผลในเครื่อง`, `กำลังตอบ`, and specific device/model failures. One turn is fenced; display the native lifecycle, minimum 250 ms hold and maximum 15 s behavior without implementing a competing browser audio lifecycle. A permission error never triggers a cloud fallback.

Mute, output volume, event-pack preview, and microphone consent are separate controls. Preempted speech and paired visuals stop together and do not resume from a stale callback. Native audio owns priority; the UI does not infer which warning should play from a score. Critical media missing means not ready, with no substitute TTS or generated event card (VOICE-001–VOICE-007, MEDIA-004–MEDIA-005, ALERT-007).

### Separate consents

Consent is specific to purpose and source. Local microphone use, optional cloud gameplay context, peer sharing, social presence, and recording each get separate explanations and explicit actions. No master “enable everything” switch, prechecked peer consent, or sign-in side effect is allowed.

Peer create/join requires a fresh checked sharing acknowledgement and the activation action together. Leaving immediately clears local peer state and fences delayed work; explain that remote expiry/delete is best effort without promising instant remote erasure. Cloud-context consent describes only its closed allowed field projection. Free-form voice, transcripts, personal memory, enemy/peer data, items, exact clock, and account identifiers never become that projection. These are display boundaries, not new payload definitions (PEER-001, PEER-008–PEER-009; ADVICE-002–ADVICE-004).

### Recording and evidence review

The recording control is initially stopped. OBS installed/connected does not mean recording is active, and starting a match does not start recording. Explain that the session is local and that Kohbor does not enable the microphone. Use the native recorder state for connecting, recording, saving, stopped, auth failure, missing OBS, and disk-related failure. Recorder failure leaves admitted local observations and alerts available.

A clip details its requested **30 s before / 15 s after** interval and actual coverage. If the saved buffer cannot prove full coverage or exact trim, label it `คลิปไม่ครบช่วงที่ขอ` with a concrete reason. Never render a generic “45 s complete” stamp from the requested interval alone. Summaries contain at most three evidence-linked points per completed session; partial sessions and unknown outcomes remain explicit (OBS-001–OBS-005; MEM-001, MEM-004).

### Base and deferred Pro

Production defaults to base unless native-verified authority exists. Base retains the field and local event feed. Eligible Pro inspector/timer examples are visibly marked as projected fixture views until their authority/contracts are implemented. There is no design-supplied price, checkout, trial, or upgrade call to action.

An invalid/lapsed/revoked capability removes Pro detail, expanded state, selection, and late responses in one coherent update. Preserve route, map bounds, own/base observations, and local feed; no modal, toast, sound, blank frame, or automatic redirect. Regain enables eligible actions without reopening selection. A valid offline lease and source freshness are evaluated independently. Presentation changes neither erase native timers nor revoke peer consent (PREMIUM-001–PREMIUM-003; UI-007).

## 9. Passive gameplay overlay

The overlay is a separate transparent, always-on-top, click-through native surface. It has no navigation rail, page header, account control, dialog, or inspector. Keep the inherited arrangement: own-stat row near top-left, risk indicator near top-center, event banner below the game's announcement area, and companion near top-right. The exact native positions and media dimensions are deliberately unclaimed until game-HUD measurements exist.

Each module has a separate enabled preference, position, scale, and opacity, with saved profiles. The control-window editor labels itself `ตัวอย่างการจัดวาง — ไม่ใช่ภาพสด`; preview edits do not pretend to be a live overlay. Preserve customized profile values across ordinary settings migrations. Reduced motion, missing observations, user-disabled modules, and unavailable sources remain distinct.

No live-data unknown may become green reassurance. A low admitted heuristic score is labelled as an estimate; no capture coverage gets a no-coverage label. Event media originates only from the active verified package revision. The overlay's smooth appearance in a browser does not establish click-through behavior, protected-area clearance, acoustic timing, FPS, CPU, or memory compliance.

## 10. Decisions, limitations, and handoff

| Design decision | v1 resolution | Remaining dependency |
| --- | --- | --- |
| DS-DEC-01 — Local clips | Insights → History → Clips filter; control on Overview; configuration in Settings → System. | Native OBS implementation and measured clip coverage. |
| DS-DEC-02 — Visual language | Nightglass: smoked graphite, mint, semantic status accents, original vector identity. | Owner review is not recorded; final native visual acceptance remains open. |
| DS-DEC-03 — Typography and density | Thai-capable stack, 16 px body / 14 px supporting, 44 px default / 36 px compact authored targets. | Measured readability and target size after fitting/DPI. |
| DS-DEC-04 — Field geometry | Exact coordinates in §5; uniform stage fit and permanent 404 px right region. | Native inner-safe-area measurement and supported-size decision. |
| DS-DEC-05 — Provenance | Explicit state grammar, visible source/age where interpretation changes, no invented zero/readiness. | Frozen producer/read-model contracts remain authoritative. |
| DS-DEC-06 — Deferred presentation | Keep required route inventory with honest unavailable states; mark Pro fixtures explicitly. | Actual service/capability authority and separate operational acceptance. |

The design artifacts resolve the delegated presentation direction and provide a reviewable implementation handoff. They do not resolve DEC-NAME technical identity, model qualification, package crypto, critical media availability, risk admission, voice buffer ownership, session completion, retention, peer service operations, premium authority, OBS combined budgets, or DEC-NATIVE-EVIDENCE. Those decisions block their named dependencies only.

## 11. References

- [UX-SPEC](UX-SPEC.md), [SRS](../requirements/SRS.md), [Decision Log](../governance/DECISION-LOG.md), and [Security and Privacy](../security/SECURITY-PRIVACY.md): internal governing behavior and data boundaries.
- [W3C Understanding SC 1.4.3 — Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html): normal-text contrast reference; accessed 2026-11-11 Asia/Bangkok.
- [W3C Understanding SC 1.4.11 — Non-text Contrast](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html): meaningful control/state/graphic contrast reference; accessed 2026-11-11 Asia/Bangkok.
- [W3C Understanding SC 2.4.7 — Focus Visible](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html): visible, persistent keyboard-focus reference; accessed 2026-11-11 Asia/Bangkok.
- [W3C Understanding SC 2.5.8 — Target Size (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html): target-size and exception reference; accessed 2026-11-11 Asia/Bangkok.

W3C Understanding pages explain the criteria. Their use here sets design checks, not a complete standards-conformance assessment or native accessibility certification.
