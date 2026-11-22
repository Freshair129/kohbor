# Test designs only: application step bindings and actual execution are still required.
# These scenarios do not claim native, live, acoustic or resource PASS.
Feature: Brand-neutral desktop companion target behavior

  @CORE-001 @mvp
  Scenario: AC-CORE-001 Account-independent local startup
    Given a cold installed application with no account and no network
    When the player starts the application
    Then local setup, permitted intake, overlay and available local functions are usable; account services show unavailable
    And failure behavior is: Do not lock gameplay functionality behind sign-in or an entitlement check.

  @CORE-002 @mvp
  Scenario: AC-CORE-002 Resource containment before UI
    Given job creation, configuration or assignment fails
    When startup is attempted
    Then the application records a safe error and exits with code 1 before WebView or sidecar creation
    And failure behavior is: Do not continue with an uncontained runtime.

  @CORE-003 @mvp
  Scenario: AC-CORE-003 Account transitions preserve local state
    Given a local match is active with an optional signed-in account
    When the account is revoked or switched
    Then protected account operations stop and late account results are discarded; valid local core continues
    And failure behavior is: Never allow an old account response to populate the new account.

  @CORE-004 @mvp
  Scenario: AC-CORE-004 Owned integration configuration
    Given ports 3000 through 3010 contain occupied and free candidates
    When setup selects an available configured port
    Then the app and owned game config agree; unrelated game configuration is preserved
    And failure behavior is: Port exhaustion or write failure returns an actionable error without overwriting unrelated files.

  @CORE-005 @mvp
  Scenario: AC-CORE-005 Supported matrix is explicit
    Given an untested mode or vision geometry is selected
    When a source-dependent conclusion would be shown
    Then the unsupported capability is identified and independent valid local capabilities remain available
    And failure behavior is: Do not reinterpret an unsupported mode as the default supported mode.

  @DATA-001 @mvp
  Scenario: AC-DATA-001 Freshness boundary
    Given a valid own observation at age 2000ms followed by age 2001ms
    When the UI consumes both snapshots without a new source observation
    Then the first may be fresh and the second is stale; invalid input and polling do not renew the age
    And failure behavior is: Withdraw live assertions when stale; do not fabricate a new observation.

  @DATA-002 @mvp
  Scenario: AC-DATA-002 Unknown is not zero
    Given HP is absent in one sample and genuinely zero in another
    When read models are built
    Then missing HP becomes unknown/null and admitted zero remains zero
    And failure behavior is: Never use parser defaults to imply alive, safe, ready or known empty.

  @DATA-003 @mvp
  Scenario: AC-DATA-003 Own-source admission
    Given a valid-looking payload belongs to another selected unit or unverified player
    When source admission is evaluated
    Then the payload cannot supply own-player or peer vitals
    And failure behavior is: Keep source unavailable; selected-unit data is not proof of ownership.

  @DATA-004 @mvp
  Scenario: AC-DATA-004 Session and clock fences
    Given an old response and regressed input arrive after a new session epoch
    When consumers evaluate pending work
    Then old work is rejected and event baselines cannot generate false new-match cues
    And failure behavior is: Preserve authoritative watermark/identity through temporary invalidity; require actual session evidence.

  @DATA-005 @mvp
  Scenario: AC-DATA-005 Enemy observation boundary
    Given game telemetry contains no enemy position and capture is unavailable
    When the live field renders enemies
    Then no current position or hidden enemy stats are invented
    And failure behavior is: Unknown coverage cannot be labeled safe.

  @DATA-006 @mvp
  Scenario: AC-DATA-006 Static game data provenance
    Given a catalog lacks a known patch or class validation
    When advice or skill UI requests those facts
    Then the facts are marked partial/unsupported and not described as latest
    And failure behavior is: No guessed damage formula or patch compatibility.

  @VISION-001 @mvp
  Scenario: AC-VISION-001 Calibrated capture admission
    Given capture changes monitor, size or minimap layout
    When a frame arrives under an old calibration
    Then the old frame/coordinates are rejected and calibration readiness is explicit
    And failure behavior is: Preserve valid telemetry-only operation.

  @VISION-002 @mvp
  Scenario: AC-VISION-002 Missing requires continuing coverage
    Given an enemy was observed and the camera then stops yielding valid frames
    When time passes beyond 5 seconds
    Then coverage becomes unavailable and no missing or all-clear assertion is manufactured
    And failure behavior is: Do not interpret empty/no frames as observed absence.

  @VISION-003 @mvp
  Scenario: AC-VISION-003 Certified class coverage
    Given an unvalidated class is detected
    When identity is presented
    Then the class is marked unknown/unverified rather than silently accepted
    And failure behavior is: Pooled model accuracy cannot certify an individual class.

  @VISION-004 @mvp
  Scenario: AC-VISION-004 Capture source separation
    Given OBS is enabled for recording
    When live vision continues
    Then the selected critical capture route remains explicit and accidental double capture is avoided
    And failure behavior is: A recorder or candidate virtual-camera failure cannot erase valid GSI.

  @ALERT-001 @mvp
  Scenario: AC-ALERT-001 Native independent warning path
    Given network is down and a renderer/model task is stalled
    When an admitted actionable critical event occurs
    Then the local alert path continues and meets its acoustic gate
    And failure behavior is: No synchronous network, disk unpacking or inference in critical admission.

  @ALERT-002 @mvp
  Scenario: AC-ALERT-002 Risk is a labeled heuristic
    Given neutral headings and durations 4,5,8.5,12,20,40 seconds
    When the baseline risk function runs
    Then outputs are 0,0,0.35,0.70,0.46,0.10 within declared floating tolerance
    And failure behavior is: Invalid/non-finite input produces unavailable rather than a risk probability claim.

  @ALERT-003 @mvp
  Scenario: AC-ALERT-003 Evidence-gated audible gank
    Given score crosses the configured danger threshold with no qualifying movement/position evidence
    When voice admission is evaluated
    Then no audible gank is emitted; a heuristic readout does not claim confirmed threat
    And failure behavior is: Suppress during pregame, own death, pause/nonadvancing clock, invalid game frame or capture gap.

  @ALERT-004 @mvp
  Scenario: AC-ALERT-004 Absence episode deduplication
    Given a notified hero remains absent after both cooldowns expire
    When a new risk evaluation occurs
    Then cooldown expiry alone does not create a new notification episode
    And failure behavior is: Re-arm only on confirmed continuous reappearance; one-frame noise does not re-arm.

  @ALERT-005 @mvp
  Scenario: AC-ALERT-005 Evidence-based correction
    Given an alert was issued and score later falls solely by time decay
    When the clear threshold is crossed
    Then no audible all-clear/correction is emitted from decay alone
    And failure behavior is: Coverage loss silently suspends unsupported claims and retains cooldown fences.

  @ALERT-006 @mvp
  Scenario: AC-ALERT-006 Missing notification policy
    Given a previously observed enemy is absent 5000ms then 5001ms under valid coverage
    When missing notification admission runs
    Then no threshold event at 5000ms; an eligible event after 5000ms obeys dedup and audio priority
    And failure behavior is: Drop missing-event audio while critical audio is active; no substituted media.

  @ALERT-007 @mvp
  Scenario: AC-ALERT-007 Critical media readiness
    Given gank/danger/revision media is missing or invalid
    When critical readiness is assessed
    Then the product exposes not-ready and the acoustic gate cannot pass
    And failure behavior is: Do not substitute TTS, an unrelated clip or silently declare visual-only readiness.

  @MEDIA-001 @mvp
  Scenario: AC-MEDIA-001 Single event taxonomy
    Given duplicate IDs, duplicate nonzero ranks or invalid tunables exist
    When catalog validation/generation runs
    Then the catalog is rejected before generated runtime tables are published
    And failure behavior is: Do not hand-edit generated downstream copies.

  @MEDIA-002 @mvp
  Scenario: AC-MEDIA-002 Ordinary event arbitration
    Given one tick produces several admitted ordinary events
    When the announcer chooses media
    Then the highest emitted ordinary rank is chosen; if its mapping is missing/invalid the slot stays silent without a lower-rank media fallback
    And failure behavior is: No mapping-driven fallback. Baseline/reconnect must not invent first blood, kills, level or match-start events.

  @MEDIA-003 @mvp
  Scenario: AC-MEDIA-003 Respawn unvoiced
    Given an admitted own death transitions to respawn
    When the detector publishes the transition
    Then history/state may reflect respawn but the announcer does not voice it
    And failure behavior is: Do not treat every event ID as requiring a sound.

  @MEDIA-004 @mvp
  Scenario: AC-MEDIA-004 Verified paired media
    Given mixed audio is corrupt while an original identity audio file is valid
    When playback is requested
    Then the event pair is rejected and a local reason is recorded
    And failure behavior is: No loose-file/default-pack/generated-card/event-TTS fallback.

  @MEDIA-005 @mvp
  Scenario: AC-MEDIA-005 One audio owner
    Given a clip has a declared visual tail
    When a critical interruption or session change occurs
    Then obsolete audio and visual tails terminate under the same generation
    And failure behavior is: The renderer cannot independently select or replay event media.

  @MEDIA-006 @mvp
  Scenario: AC-MEDIA-006 Bounded hostile package handling
    Given an archive has unsafe paths, duplicate records or expansion beyond limits
    When installation validates the archive
    Then it fails in bounded staging with no active-registry mutation
    And failure behavior is: Cap container/expanded total 512MiB, payload 32MiB, descriptor/registry 2MiB and payload count 2048.

  @MEDIA-007 @mvp
  Scenario: AC-MEDIA-007 Atomic install versus activation
    Given a valid installation commits while a match/media lease prevents selection
    When activation is requested
    Then the receipt distinguishes installed from activation deferred; the old active revision remains usable
    And failure behavior is: A later activation error cannot misreport the successful install as active.

  @MEDIA-008 @mvp
  Scenario: AC-MEDIA-008 Trust domains are separate
    Given a locally hash-valid package supplies its own public key or claims Store authority
    When the installer evaluates trust
    Then local integrity alone grants neither trusted publisher nor purchase access
    And failure behavior is: Use application-owned trusted keys and newly selected signature namespace.

  @UI-001 @mvp
  Scenario: AC-UI-001 Independent control and overlay
    Given the player changes control-window route
    When gameplay remains active
    Then overlay visibility/focus behavior stays independent and does not cover protected game areas
    And failure behavior is: Native safe-area/focus behavior needs measured evidence.

  @UI-002 @mvp
  Scenario: AC-UI-002 Session monitor field topology
    Given frozen data and viewport while inspector/feed/tier changes
    When layout transitions run
    Then map x/y/width/height remain within 1 CSS px, the map stays the largest region, and no page-level scroll is introduced
    And failure behavior is: Use bounded subregion scrolling; do not shrink text to hide overflow.

  @UI-003 @mvp
  Scenario: AC-UI-003 Neutral production start
    Given no admitted game state is available
    When the live view opens
    Then waiting/unknown state is visible and inspector remains neutral
    And failure behavior is: Fixture play/reset/seed controls cannot ship as production authority.

  @UI-004 @mvp
  Scenario: AC-UI-004 Compact unknown cards
    Given no admitted skills/items exist for a visible card
    When the card renders
    Then it shows admitted portrait/visibility/age with unknown age as an em dash
    And failure behavior is: Do not create four question-mark skill tiles or infer values from account/premium.

  @UI-005 @mvp
  Scenario: AC-UI-005 Selected inspector and ownership
    Given an ability comes from an unverified selected unit
    When the user opens the corresponding hero view
    Then unowned ability data is not exposed; selection is fenced by session/slot/identity
    And failure behavior is: Cooldown zero is not proof of castability; distinguish passive/unlearned/unknown/stale.

  @UI-006 @mvp
  Scenario: AC-UI-006 Keyboard and focus
    Given inline inspector details are expanded
    When Escape is pressed twice
    Then first close inline detail then inspector, restoring control focus safely
    And failure behavior is: Do not steal focus from the game or announce every timer tick.

  @UI-007 @deferred
  Scenario: AC-UI-007 Native estimated timers
    Given game clock pauses or becomes stale
    When wall time passes
    Then timers freeze and cannot become observed-ready from browser time
    And failure behavior is: Reviewed enemy ULT/BKB/TP timer scope, at most 15 keys; arbitrary fixture timers remain tests.

  @UI-008 @mvp
  Scenario: AC-UI-008 Peripheral threat salience
    Given the session monitor shows a live field with valid capture
    When a critical warning is admitted, then revised, and capture later becomes stale
    Then the map frame and banner enter the critical state within the same admission, clear on revision or expiry, and the stale state dims and labels the map instead of keeping live markers
    And failure behavior is: Routine updates never flash or pulse; no motion is derived from predicted paths, decay or unobserved data; reduced motion replaces animation with static emphasis.

  @UI-009 @extension
  Scenario: AC-UI-009 Edge quick panel
    Given a supported Windows borderless live or bot match with Dota foreground and the orb enabled
    When across normal live or bot matches the tester uses the orb or panel and exercises hover, protected regions, focus loss and Settings, and admits a qualified critical alert while the panel is open
    Then at least 30 orb or panel interactions are logged with zero activation_detected; only guarded hit regions receive orb input and the outer 8 px pass clicks through. No orb or panel pixel intersects PROTECTED_REGIONS. The panel closes and the orb locks in the alert admission while the cue meets its acoustic deadline. Unknown stays unknown, the orb hides outside a match or in exclusive fullscreen, and an induced activation closes the panel, restores click-through and hides the orb until the match ends. Settings can disable the orb and each panel control has a keyboard-reachable deck twin
    And failure behavior is: A qualified critical alert closes the panel and locks the orb in the same admission without delaying the critical cue. Unknown status is never shown as ready. Log activation, close the panel and restore click-through immediately. Hide the orb if click-through cannot be guaranteed.

  @UI-011 @extension
  Scenario: AC-UI-011 Orb sound mixer
    Given a supported match, three session states, and mixer settings including Focus level and optional hotkey
    When the player adjusts, mutes, focuses, restores, uses a wheel and encounters missing or failed sessions
    Then each present channel exposes its actual 0 to 100% value and mute, Focus applies the selected level and restores the prior value, the hotkey is inactive unless enabled, wheel steps by 5% only with panel open, failures are honest, and every control has a deck twin
    And failure behavior is: Never show a missing or unreadable session as controllable. On set failure restore the last read value and state the reason. Preserve Dota focus and critical-alert priority.

  @UI-010 @extension
  Scenario: AC-UI-010 Display-only hero pick card
    Given an admitted draft, deck-selected position and bracket, and supported 1920×1080 layout
    When the player picks or changes hero, advances through selection, strategy and showcase, and uses builds with and without admitted 5b output
    Then the single column occupies x=1728–1907 scaled with height and shows the read-only list before own pick and existing pick card after it; the list shows historical bracket win rate and game count, labels under-1,000 samples low-data and five position groups as estimates, discloses that listed heroes may be banned or already picked, never shows win-chance %, accepts no in-game input, and has its own on/off switch; v2 only strikes picked heroes when 5b is available, while matchup edge remains unavailable pending a qualified source
    And failure behavior is: Hide without an admitted own pick, a supported placement or an active draft phase. Suppress v2 comparisons when 5b is unavailable and keep eligible v1 content. Remove previous hero statistics immediately on a repick or swap.

  @UI-016 @mvp
  Scenario: AC-UI-016 Second-monitor Live layout
    Given Live boxes for 1920×1080 and 2560×1440 monitors with monitor 2 on each side, a narrowing window and a window below 1100×700
    When the player sets the side, resizes the window, opens the own-hero expander and enters or leaves a match
    Then the map is pinned to the inner edge at the size the box allows, the quick strip matches its width, outer columns collapse before the map shrinks, the page falls back below the threshold with a note, rails move outward and the friends rail folds only during a match, and every control is keyboard-reachable, and no status chip or control text overlaps another at the tested widths
    And failure behavior is: Without a stored or valid side, or when the window is too small, show the centred deck. Never move or resize the map because of panel content, feed changes or the expander. Status chips and controls in the outer columns wrap to a new line instead of shrinking under their text; no label may paint over another (owner report on build 8d9f8c3, 2026-11-23).

  @UI-017 @mvp
  Scenario: AC-UI-017 Second-monitor content from admitted sources
    Given a session with and without admitted draft identities, a known and unknown own team, risk counts present and absent, no objective source, own GSI abilities, ally ultimate-dot inputs present and absent, and peer data present and absent
    When the second-monitor Live page renders
    Then both teams show hero cards with portrait, name, HP, mana and four skill squares; names come only from same-session draft memory and the own hero is not repeated; own values come from GSI; ally HP, mana and cooldowns appear only with peer data; without it the ally ultimate square shows only the dot state with no number; enemy vitals and skills are em dashes; missing-from-map is shown only when both counts are known; each value carries its source label; and no abnormal states appear
    And failure behavior is: Unknown is an em dash, never zero, never a guessed side and never a guessed cooldown. If the own team or own hero cannot be matched, the ally cards stay unknown and no ultimate-dot state is bound to a guessed ally. An unrecognised dot value is unknown.

  @UI-018 @mvp
  Scenario: AC-UI-018 Second-monitor rune countdowns on the minimap
    Given a host schedule with water, power, bounty and wisdom occurrences, lead times set and unset, a stale schedule and a malformed one
    When the pinned minimap renders across the game clock
    Then each rune spot shows minutes until the largest lead time, then seconds to 0; shared spots show the sooner objective; stale, malformed or missing schedules show nothing; Tormentor and Roshan are never labelled; markers are original
    And failure behavior is: No schedule, a malformed schedule or one older than five seconds shows no labels and an em dash in the quick strip. Spot positions are approximate placement for patch 7.42 and are not measurements; a wrong spot never changes a time.

  @UI-019 @mvp
  Scenario: AC-UI-019 Second-monitor broadcast-style draft board
    Given a draft with some heroes identified, the own team known and unknown, names present with the setting on and off, and statistics present, absent and out of range
    When the second-monitor Live page renders during the draft and after it
    Then the map square shows two team rows of five slots; identities come only from same-session draft memory; names appear only with the setting on; our side shows an estimated win rate and position, the enemy side a win rate and a guessed lane; no win chance appears; unknown values are em dashes; and the minimap returns after the draft
    And failure behavior is: An unidentified hero, a missing name, a name while the setting is off, or a missing or out-of-range statistic shows an em dash. Statistics are shown only for identified heroes. Low-data statistics carry a low-data label.

  @MEDIA-009 @extension
  Scenario: AC-MEDIA-009 Per-application session volume
    Given a fake session manager with present, missing and unreadable sessions and persisted original levels
    When the host sets, restores, crashes and restarts, and the default device or session changes
    Then only eligible Dota and selected music sessions change, originals restore on exit/disable/reset and after crash restart, changed devices/sessions are re-read before use, logs contain only the permitted fields, and no process input, memory or files are accessed; on the owner PC lower Dota and Chrome, kill Kohbor, restart and confirm original levels restore
    And failure behavior is: Do not change an unreadable, missing or unselected process. On restore failure report it honestly and retain recovery data for the next start.

  @STATS-001 @extension
  Scenario: AC-STATS-001 Separate consent for public hero statistics
    Given a fresh profile, separate account and consent states, and a controllable provider with a seeded stats cache after opt-in
    When the player signs in, opens a draft and changes unrelated consents before explicitly opting in, then revokes while a request is pending
    Then only the dedicated switch enables the route. Revocation stops all new sends, discards the late response, clears displayed statistics and deletes the local stats cache while local core remains available
    And failure behavior is: Keep sends and cache access disabled after revocation, including on retry or restart. A late response shall not restore display or cache. Report a cache deletion error accurately and keep the revoked cache inaccessible.

  @STATS-002 @extension
  Scenario: AC-STATS-002 Closed outgoing public statistics projection
    Given public hero statistics consent is on and test data contains the own hero, an optional manual bracket and forbidden fields
    When the host constructs and sends a valid request, then receives negative cases with an extra field including each forbidden data class
    Then v1 list requests are fixed public table requests, no more than four per day, with no user- or match-specific fields; position and bracket filtering occurs locally. The separate pick-card flow continues to carry only the admitted own hero ID and optional manual bracket, and forbidden context is rejected before send
    And failure behavior is: Reject requests with extra or invalid fields before transport. Do not serialize a wider game, account or draft object or substitute a missing own hero ID.

  @STATS-003 @extension
  Scenario: AC-STATS-003 Honest public statistics values and states
    Given minimum samples have been fixed and fixtures cover valid and missing metadata, stale data, every unavailable state and sample sizes around the minima
    When the host evaluates provider responses, cache expiry, a new hero and consent transitions
    Then each shown value carries source, window or unspecified label, fetch time and its sample size, every state is distinct, and insufficient or expired data produces no unsupported value. No invented value, silent fallback or win-chance percentage appears; the suggestions list shows source and sample, low-data below 1,000 games, labels estimated five-position groups, discloses possible banned/picked heroes, shows no win-chance percentage, and never fills unknown provider metadata
    And failure behavior is: Withhold unsupported or undersampled values. Use eligible cached values only with explicit age and stale labelling when applicable. Never silently fall back, invent a number, reuse another hero's results or interpret missing data as zero.

  @VOICE-001 @mvp
  Scenario: AC-VOICE-001 Explicit microphone consent
    Given mic consent is false
    When PTT or account/cloud/peer action occurs
    Then no microphone stream opens
    And failure behavior is: Other consent and account sign-in never enable mic.

  @VOICE-002 @mvp
  Scenario: AC-VOICE-002 Remappable conflict-safe PTT
    Given a new binding cannot be registered
    When the user changes the shortcut
    Then the prior working binding/settings remain and error is visible
    And failure behavior is: Do not silently remap or steal reserved keys; Alt+M remains mute.

  @VOICE-003 @mvp
  Scenario: AC-VOICE-003 One bounded voice turn
    Given a turn is decoding and a new valid press arrives
    When the new turn starts
    Then the prior turn is cancelled; unmatched release and repeat held press do not create work
    And failure behavior is: Late callbacks cannot resume the superseded request.

  @VOICE-004 @mvp
  Scenario: AC-VOICE-004 Bounded transient audio
    Given device opening fails or a turn is cancelled
    When capture exits
    Then stream/resources are released and transient data is zeroed/dropped within the frozen handoff contract
    And failure behavior is: No persistence or indefinite decoder retention; STT ownership/lifetime must be closed before integration.

  @VOICE-005 @mvp
  Scenario: AC-VOICE-005 All-local conversation
    Given local STT or answer provider is unavailable
    When the player finishes PTT
    Then the product reports unavailable and preserves critical local alerts
    And failure behavior is: Never fall back to cloud or log raw speech/text.

  @VOICE-006 @mvp
  Scenario: AC-VOICE-006 Priority preemption
    Given a Voice turn is listening/decoding/answering/speaking
    When a qualified critical alert is admitted
    Then the turn is cancelled and the urgent cue meets its acoustic deadline
    And failure behavior is: No stale answer or visual tail may resume later. Ordinary announcer preemption also fences the superseded voice answer.

  @VOICE-007 @mvp
  Scenario: AC-VOICE-007 Honest preparation and failure
    Given a selected local model is cold or absent
    When a turn requests inference
    Then the UI shows preparing or unavailable with bounded completion behavior
    And failure behavior is: Do not hide failed/rejected turns from completion/latency statistics.

  @MEM-001 @mvp
  Scenario: AC-MEM-001 Local evidence-supported aggregates
    Given an archive lacks explicit outcome or death-cause facts
    When the memory reader builds a snapshot
    Then those facts remain unknown and no fabricated winrate/death narrative appears
    And failure behavior is: Peer data and model guesses cannot become observed memory.

  @MEM-002 @mvp
  Scenario: AC-MEM-002 Atomic versioned snapshots
    Given a snapshot rebuild fails halfway
    When the next reader opens memory
    Then the prior valid snapshot or explicit unavailable state is used; no partial snapshot is admitted
    And failure behavior is: Changing storage engines needs migration and budget evidence.

  @MEM-003 @mvp
  Scenario: AC-MEM-003 Durable forget cutoff
    Given older local logs remain after a memory clear
    When the application restarts and rebuilds memory
    Then sources at or before cutoff cannot be re-learned; cutoff cannot move backward
    And failure behavior is: Corrupt cutoff fails closed. Do not persist a process-monotonic timestamp across restarts; conservatively exclude same-granularity source timestamps.

  @MEM-004 @mvp
  Scenario: AC-MEM-004 Bounded source-linked summaries
    Given a session is incomplete or an outcome is unobserved
    When summary generation runs
    Then partial/unknown is shown and win/loss/death cause are not inferred
    And failure behavior is: Do not call a model narrative a proven event.

  @MEM-005 @mvp
  Scenario: AC-MEM-005 Separate history deletion
    Given the player chooses forget memory
    When clear executes
    Then derived memory is forgotten and unrelated archives remain
    And failure behavior is: Automatic archival retention/deletion needs an explicit rule.

  @ADVICE-001 @mvp
  Scenario: AC-ADVICE-001 Required local/manual advice
    Given cloud consent is off and local inference is ready
    When manual advice is requested
    Then only permitted local context is used and critical warnings retain priority
    And failure behavior is: Unavailable local provider produces unavailable, not a silent remote request.

  @ADVICE-002 @mvp
  Scenario: AC-ADVICE-002 Independent cloud consent
    Given the user signs in, selects a provider, enters a key or presses Ask
    When cloud advice dispatch is evaluated
    Then none of those actions alone grants context consent
    And failure behavior is: Block the send and offer local/unavailable.

  @ADVICE-003 @mvp
  Scenario: AC-ADVICE-003 Closed cloud projection
    Given a local state also contains enemy data, items, exact clock, memory, voice or account identifiers
    When cloud projection is built
    Then only the closed allowed fields pass and extra fields are rejected
    And failure behavior is: Never serialize the entire game/local context and redact afterward.

  @ADVICE-004 @mvp
  Scenario: AC-ADVICE-004 Consent and session cancellation
    Given a consented request is in flight
    When consent is revoked before response delivery
    Then future sends stop and the old response is discarded
    And failure behavior is: Retries, CLI/API and error paths cannot bypass the latest consent generation.

  @PEER-001 @extension
  Scenario: AC-PEER-001 Fresh opt-in join
    Given peer consent is false or an invitation is accepted without new checked consent
    When native join is requested
    Then the request is rejected before networking
    And failure behavior is: No autojoin, saved room credentials or view-only mode.

  @PEER-002 @extension
  Scenario: AC-PEER-002 Strict source admission
    Given one required source condition is missing or invalid JSON arrives
    When publication is evaluated
    Then prior publication is withdrawn and no new sample is sent
    And failure behavior is: Do not replay an older valid source until a new admitted observation arrives.

  @PEER-003 @extension
  Scenario: AC-PEER-003 Narrow complete vitals
    Given a sample omits alive, has HP above max or includes ability/item/position
    When schema and semantic admission run
    Then the sample is rejected; unknown is not defaulted
    And failure behavior is: Ability/item sharing requires a new protocol and consent decision.

  @PEER-004 @extension
  Scenario: AC-PEER-004 Bounded membership
    Given owner leaves or membership exceeds its timeout
    When relay state advances
    Then the appropriate membership/room closes and clients require explicit new join
    And failure behavior is: Invite rotation invalidates future old-code joins but does not itself kick current members.

  @PEER-005 @extension
  Scenario: AC-PEER-005 Conservative peer age
    Given an observation is repeatedly polled without a new game sample
    When relay/native/UI age calculation advances
    Then polling never rejuvenates it; 2000ms may be live and 2001ms is stale/null
    And failure behavior is: Do not compare wall clocks across devices or retain live vitals when IPC stops.

  @PEER-006 @extension
  Scenario: AC-PEER-006 Bounded network worker
    Given a transport timeout occurs
    When retry logic runs
    Then bounded 0.5–5s retry and successful null probe precede resuming samples
    And failure behavior is: Use 1500ms request timeout, 32KiB reply cap and normal 500ms cadence; no task per game tick.

  @PEER-007 @extension
  Scenario: AC-PEER-007 Native credentials and trusted origins
    Given a renderer asks for arbitrary origin/path/redirect or member credentials
    When transport authorization runs
    Then untrusted request is rejected and tokens stay native
    And failure behavior is: No URL credentials/query/fragment, redirects or token logging.

  @PEER-008 @extension
  Scenario: AC-PEER-008 Leave fences old work
    Given a join/sync response is delayed
    When the player leaves before it arrives
    Then late data cannot reconnect or repopulate the room
    And failure behavior is: Remote delete is best effort; report expiry behavior without promising instantaneous remote erasure.

  @PEER-009 @extension
  Scenario: AC-PEER-009 No peer persistence or AI
    Given memory/history/advice attempts to consume a peer sample
    When data routing is checked
    Then the route is denied and samples are not written to logs or AI
    And failure behavior is: Friendship/premium/cloud consent cannot expand recipients.

  @PEER-010 @extension
  Scenario: AC-PEER-010 Separate-scope ability and cooldown sharing
    Given a teammate shares vitals but has not granted the abilities scope, and another teammate has granted both
    When the receiver renders both allied cards
    Then the first shows vitals with a not-shared skill row and no ability payload was transmitted; the second shows only admitted ability levels and cooldowns with their age, and withdrawing the scope clears them
    And failure behavior is: Without the abilities scope nothing is sent or shown; unknown cooldown stays unknown; cooldown zero is not shown as ready; no item, position, enemy or selected-unit data.

  @ACCOUNT-001 @extension
  Scenario: AC-ACCOUNT-001 External-provider sign-in
    Given a user starts sign-in with the configured external identity provider
    When the provider hand-off returns through the loopback redirect
    Then the flow uses the system browser and PKCE, uses one provider, opens no embedded webview, and collects no credentials in Kohbor
    And failure behavior is: A flow using multiple providers, an embedded webview, or Kohbor-entered credentials fails.

  @ACCOUNT-002 @extension
  Scenario: AC-ACCOUNT-002 Minimal provider identity storage
    Given an external identity provider has authenticated a user
    When the account service stores the login identity
    Then it stores a keyed hash of the provider subject and stores no email, profile or provider token
    And failure behavior is: The requirement fails if service storage contains the raw provider subject, email, profile data or a provider token.

  @ACCOUNT-003 @extension
  Scenario: AC-ACCOUNT-003 Separate account identity layers
    Given an account has a login identity, account id, friend code and display name
    When identity is disclosed during an account or social operation
    Then the three identity layers remain separate and only the friend code and display name reach the owner's chosen counterparts
    And failure behavior is: The requirement fails if the account id, login identity or any other identity value is disclosed, or disclosure reaches anyone beyond the owner's chosen counterparts.

  @ACCOUNT-004 @extension
  Scenario: AC-ACCOUNT-004 Private rotatable friend codes
    Given an account is issued a friend code
    When the code is inspected and then rotated
    Then the code is random, non-sequential, checksummed, rotatable and uses a Kohbor-only format
    And failure behavior is: A sequential, unchecked, non-rotatable or non-Kohbor friend code fails validation.

  @ACCOUNT-005 @extension
  Scenario: AC-ACCOUNT-005 Protected Kohbor sessions and cache clearing
    Given a user has a Kohbor account session and cached account data
    When the session is renewed, then the user signs out or switches accounts
    Then the session is short-lived, rotating, device-bound and stored under OS user-scoped protection, and all cached account data is erased
    And failure behavior is: A session that is not Kohbor-issued, short-lived, rotating, device-bound and OS user-protected fails; cached account data remaining after sign-out or switch also fails.

  @ACCOUNT-006 @extension
  Scenario: AC-ACCOUNT-006 Offline account independence
    Given the device has no network or the account service is unavailable
    When the application starts and the user uses local functions
    Then startup and every local function are unaffected and the account surface reports offline or unavailable without prompting
    And failure behavior is: Network or account-service unavailability must not interrupt startup or any local function, or prompt the user.

  @ACCOUNT-007 @extension
  Scenario: AC-ACCOUNT-007 Product-scoped token authority
    Given a token or ticket is presented for account or relay authority
    When its issuer and audience are checked
    Then only a Kohbor issuer with one audience is accepted and tokens from another product or audience are rejected
    And failure behavior is: A token or ticket with an issuer other than Kohbor, or with an audience other than its single authorized audience, is rejected.

  @ACCOUNT-008 @extension
  Scenario: AC-ACCOUNT-008 In-app account export and deletion
    Given a user has an account and can access its controls in the app
    When the user exports and then deletes the account
    Then the account can be exported; deletion removes every server record and local cache and writes one ledger entry with only the keyed login-identity hash and deletion time; after a backup restore, replay removes the restored account but not a newer account with the same login hash; the entry is purged after 30 days once no earlier backup remains
    And failure behavior is: Export or deletion unavailable in the app, any server record or local cache remaining after deletion, a ledger entry holding anything beyond the keyed login-identity hash and deletion time, a ledger stored inside the restored database, a replay that deletes an account created after the recorded deletion, or an entry kept past 30 days once no earlier backup remains, or purged while one does, fails.

  @ACCOUNT-009 @extension
  Scenario: AC-ACCOUNT-009 No unapproved platform identity binding
    Given DEC-STEAM-BINDING is undecided or a binding is enabled under a decided record
    When a platform identity is considered for an account
    Then none is bound while undecided; any bound identity has provider verification and a local match and is not a login, recovery credential, search key or displayed value
    And failure behavior is: A platform identity bound before DEC-STEAM-BINDING is decided, without both proofs, or used as a login, recovery credential, search key or displayed value fails.

  @ACCOUNT-010 @extension
  Scenario: AC-ACCOUNT-010 No cross-product account sharing
    Given a Kohbor account and another product account exist or a link is proposed
    When account data and trust material are handled or the cross-product link is attempted
    Then nothing is shared with or imported from the other product and a link proceeds only after its own decision and consent
    And failure behavior is: Any account, identifier, key, secret or endpoint shared with or imported from another product, or any cross-product link without its own decision and consent, fails.

  @SOCIAL-001 @extension
  Scenario: AC-SOCIAL-001 Optional account boundary
    Given a signed-out user opens core or joins a guest room
    When account checks run
    Then core and permitted guest flow remain available; account-only social actions are unavailable
    And failure behavior is: No public social directory or implicit platform-friend import.

  @SOCIAL-002 @extension
  Scenario: AC-SOCIAL-002 Friendship is not sharing consent
    Given two users become friends and presence is enabled
    When a targeted invitation arrives
    Then a new checked peer-sharing action is still required before joining
    And failure behavior is: Presence means app online only; avoid public last-seen/history.

  @SOCIAL-003 @extension
  Scenario: AC-SOCIAL-003 Transactional invitation authority
    Given a block/revoke races an invitation or admission
    When authorization resolves
    Then the frozen server transaction order rejects stale authority; no auto guest fallback occurs
    And failure behavior is: Tokens/account IDs do not leak to a relay; production awaits full private design.

  @OBS-001 @mvp
  Scenario: AC-OBS-001 Explicit local media session
    Given OBS is installed but recording has not been requested
    When the desktop app starts or enters a match
    Then no recording/replay/microphone starts automatically
    And failure behavior is: Credentials remain local; no cloud/stream upload.

  @OBS-002 @mvp
  Scenario: AC-OBS-002 Observe before audio suppression
    Given two admitted events share a tick and only one is voiced
    When highlight eligibility is evaluated
    Then the quiet/suppressed eligible event can still schedule a clip
    And failure behavior is: Wrong-owner/stale/out-of-session events cannot qualify.

  @OBS-003 @mvp
  Scenario: AC-OBS-003 Measured clip coverage
    Given the buffer cannot cover the full requested interval
    When a clip is saved and indexed
    Then actual coverage and partial reason are reported; full coverage is not fabricated
    And failure behavior is: Saving the entire OBS buffer is not proof of exact trimming.

  @OBS-004 @mvp
  Scenario: AC-OBS-004 Recorder failure isolation
    Given OBS stops while valid telemetry/alerts run
    When recording operations fail
    Then status shows recorder unavailable while local alerts keep functioning
    And failure behavior is: Bound pending saves; no blocking trim on the critical path.

  @OBS-005 @mvp
  Scenario: AC-OBS-005 Recording-mode evidence
    Given recording is active during a validation profile
    When CPU/RAM/GPU/FPS and alert timings are measured
    Then receipts separate app processes from OBS and report combined totals without inventing a combined cap
    And failure behavior is: No assertion that the external recorder belongs to the app Job Object.

  @UPDATE-001 @mvp
  Scenario: AC-UPDATE-001 Trusted updater authority
    Given renderer input attempts an arbitrary update URL or signature is invalid
    When update verification runs
    Then installation is refused and valid local core remains usable
    And failure behavior is: Network failure never permits unsigned installation.

  @UPDATE-002 @mvp
  Scenario: AC-UPDATE-002 Safe installation timing
    Given a verified candidate is available during a match
    When installation is requested
    Then it is deferred/staged without interrupting active gameplay
    And failure behavior is: An update prompt cannot force focus or local-core lock.

  @UPDATE-003 @mvp
  Scenario: AC-UPDATE-003 Promote identical signed bytes
    Given a candidate is ready for stable promotion
    When release preparation runs
    Then hash and signature match the approved candidate and no rebuild/resign occurs
    And failure behavior is: Tagging/normal commit alone does not publish; failed candidate cannot mutate stable.

  @UPDATE-004 @mvp
  Scenario: AC-UPDATE-004 Verified recovery route
    Given a failed update needs recovery
    When the documented rollback route is tested
    Then a known approved compatible artifact or new higher emergency version restores operation
    And failure behavior is: Do not assume a manifest downgrade will be accepted.

  @PREMIUM-001 @deferred
  Scenario: AC-PREMIUM-001 Native trusted capability lease
    Given a renderer/local fixture claims premium without a valid grant
    When capability admission runs
    Then base remains active and protected details are not enabled
    And failure behavior is: Production issuing/key/RLS/device design is deferred.

  @PREMIUM-002 @deferred
  Scenario: AC-PREMIUM-002 Quiet loss and regain
    Given a selected premium detail is open
    When authority is lost then later regained
    Then detail/late responses clear quietly; map/feed remain; regain does not reopen selection
    And failure behavior is: Offline with a still-valid signed lease retains capabilities.

  @PREMIUM-003 @deferred
  Scenario: AC-PREMIUM-003 No premium data or safety expansion
    Given base user is in a valid match and a premium user lacks peer consent
    When features evaluate access
    Then base alerts/GSI/overlay work and premium cannot access unconsented or unavailable fields
    And failure behavior is: A paid tier is not evidence of game data availability.

  @QUALITY-001 @mvp
  Scenario: AC-QUALITY-001 Acoustic warning latency
    Given an instrumented scenario records acquisition-due or GSI-arrival T0 and loopback audible T3
    When latency acceptance is evaluated
    Then T3−T0 includes scheduling/acquisition/inference waits and meets both limits
    And failure behavior is: Do not move T0 after frame-ready or replace hard maximum with p99.

  @QUALITY-002 @mvp
  Scenario: AC-QUALITY-002 Whole-app CPU and memory
    Given host, WebView, sidecars and active external model processes are measured
    When warmup and idle/live/voice-alert profiles run
    Then all included process samples satisfy the fixed CPU limit with raw receipts; app-group full Working Set and, when the GPU is used, VRAM are measured per process and reported with raw receipts for the idle, live and voice-alert profiles, as the baseline from which the owner sets a limit
    And failure behavior is: No excluding model/shared pages or substituting private/PSS/forced trim. A RAM or VRAM value that was not measured is unknown, never zero or within budget. No RAM or VRAM limit is applied or claimed until the owner sets one.

  @QUALITY-003 @mvp
  Scenario: AC-QUALITY-003 FPS preservation
    Given paired runs use identical validated conditions
    When FPS impact is calculated per pair
    Then every pair passes and evidence includes active recording where applicable
    And failure behavior is: A pooled average cannot hide a failing pair.

  @QUALITY-004 @mvp
  Scenario: AC-QUALITY-004 Vision quality per class
    Given at least 50 episodes/class in at least 5 held-out matches and 20 whole held-out matches exist
    When independent quality evaluation runs
    Then per-class results, 95% confidence intervals and disagreement are reported
    And failure behavior is: No train/holdout leakage or pooled substitution.

  @QUALITY-005 @mvp
  Scenario: AC-QUALITY-005 Warning quality
    Given independently labeled threat episodes are fixed before tuning
    When each sensitivity profile is evaluated after dedup
    Then results meet each threshold with attribution/evidence
    And failure behavior is: Death alone is not ground truth and no coverage cannot be safe.

  @QUALITY-006 @mvp
  Scenario: AC-QUALITY-006 Voice quality and speed
    Given at least 100 held-out utterances per language cover quiet/game-audio and separate speakers/sessions
    When local STT/answer is evaluated
    Then English WER ≤15%, Thai CER ≤20%, intent/critical-slot accuracy ≥90% and latency/completion pass
    And failure behavior is: Count failures/timeouts/rejects; synthetic scorer tests do not certify a model.

  @QUALITY-007 @mvp
  Scenario: AC-QUALITY-007 Preemption under overlap
    Given each voice lifecycle stage overlaps critical admission
    When priority/cancellation trials execute
    Then every trial preempts obsolete output and meets the alert deadline
    And failure behavior is: Report failed/cancelled trials without pruning the denominator.

  @QUALITY-008 @mvp
  Scenario: AC-QUALITY-008 Evidence-gated certification
    Given a required cell lacks device or acoustic evidence
    When release readiness is assessed
    Then the cell is SKIP/INDETERMINATE and the mandatory gate remains unsatisfied
    And failure behavior is: A unit test, browser screenshot or commit does not substitute for the missing evidence.

