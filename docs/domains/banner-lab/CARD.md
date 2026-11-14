---
domain: banner-lab
tier: A
card_version: 2
verified_at_commit: d9db064
verified_on: 2026-11-14
verified_by: "claude-code subagent draft (read-only), integrated by the governance lead; agent work, not human review"
---

# Card: banner-lab

A map, not evidence. Authority order: decisions, PRD/SRS, contracts, code and tests, then this card. `lab:` is the logical root of the banner-lab workspace, which lives outside the repository and is not a git repository (mapping: coordination ledger, `registry/branches.json`, key `logical_roots`). Registry rows `banner-studio` and `announcer-voice` (tier C) are answered here.

## 1. Purpose and boundary

Production lab for event banners (silent WebM with alpha plus a mixed WAV) and for the synthetic announcer voice used by them. The lab is a producer; Kohbor only consumes finished files through the package tool.

- Owns: `lab:fable/**` (the baseline lab and its design system), `lab:voice/**` (announcer-voice pipeline, internal), `lab:studio-mockups/**` and `lab:STUDIO-DESIGN*.md` (banner-studio: design material only, no application code exists), the disposable private copies `lab:fable-*/` and `lab:nk-*/`.
- Workers (owner decision 2026-11-14, DEC-BANNER-WORKER): the default banner BUILD worker is `gpt-6.1-sol` (profile `sol-xhigh`); `astra-high` is kept for UI design mockups only and is not the banner default; `luna-max` stays the implementation worker; a build by Sol is reviewed by `terra-high`, never by Sol. Taste is not decided by any worker (docs/governance/AGENT-REGISTRY.json).
- Read-only here: `lab:astra/` (competing lab, not adopted), `lab:nakom-src/` (owner-supplied inputs), `lab:workflow-design/` (design owned by governance), `lab:BRIEF.md` (original contract, facts).
- Must not touch: anything inside the repository; any package, release or store; shipped lab files outside an owner-approved build (`lab:AGENTS.md` sections 3, 4); the voice clips (mix at one fixed gain only).
- Interfaces: `packs` (the only entry from the lab into the repo is `kohbor-pack-build` in `crates/kohbor-pack-author`; the tool refuses an output directory inside a git worktree); `events-audio` (the lab reads `contracts/event-catalog.json`, never edits it; an unbound banner is "asset only" with a reason); `ui-overlay` (banner look is not a DEC-VISUALS token; overlay playback of the clips is untested); `governance` (claims, card write-back).
- Voice boundary: the repository `AGENTS.md` excludes real-person voices and source-brand assets from the product. Voice, voice profile and every output that contains the voice stay outside the repository and outside any package. Rights: owner statement, unverified; handled by the owner's rights team; agents record, never decide.
- Reading order inside the lab: `lab:AGENTS.md`, then `lab:fable/DESIGN-SYSTEM.md` before touching a banner.

## 2. Owner decisions (ids only; text lives in contracts/decisions.json)

- Decided: DEC-BANNER-LAB-STYLE, DEC-BANNER-WORKER, DEC-PACK-AUDIO-OPTIONAL-VISUAL, DEC-COMMERCE-INTENT
- DEC-BANNER-LAB-STYLE does NOT approve: DEC-VISUALS tokens, any clip as critical media (DEC-CRITICAL-MEDIA), rights to a voice, shipping any banner. Pipeline in one line: a WebGL2 renderer in headless Edge piped to a pinned ffmpeg, rebuilt by one command (`lab:fable/build.mjs`; `--only` and `--skip-pack` exist), then `kohbor-pack-build`.
- Open: DEC-VISUALS, DEC-CRITICAL-MEDIA, DEC-EVENT-RUNE-SPAWN
- Proposed: DEC-EVENT-AUDIO
- Deferred: DEC-PACK-BANNER-ADDON

## 3. Now / Next / Blocked

- Now: Baseline `lab:fable/` has 10 entries in `lab:fable/src/banners.json`: 6 bound to catalog events, 4 asset-only (listed in `lab:fable/report.json`, key `not_in_mapping`). DEC-BANNER-LAB-STYLE
- Now: The baseline still renders the old system font; the open-licence font swap exists only in private copies (phase A), phase B re-render is not approved. `lab:AGENTS.md` section 6
- Now: Ledger claims active, both expiring 2026-11-14 (the claim does not release the area by itself): `banner-lab__nakom-taste-test`, `banner-studio__ux-mockups`
- Now: Banner Studio is a proposal only (increments 1 to 6, owner decides scope and pack count); no code. `lab:STUDIO-DESIGN.md`
- Now: The pack tool binary the lab runs is dated 2026-11-12 18:44, before commit 8b70f6f (audio-only bindings); whether it contains that change is UNVERIFIED. DEC-PACK-AUDIO-OPTIONAL-VISUAL
- Next: Owner listens to every mixed WAV and confirms every Thai sentence (agents cannot). DEC-BANNER-LAB-STYLE
- Next: Play a banner in the real overlay (VP9 alpha) and measure decode cost; tiers 4, 5 and the streak family are not built. DEC-BANNER-LAB-STYLE
- Next: Icon docked onto the plaque (owner preference, not built) needs a recorded amendment of the style decision first. DEC-BANNER-LAB-STYLE
- Blocked: `rune_spawn` has no catalog entry and no detector; stays asset-only. DEC-EVENT-RUNE-SPAWN
- Blocked: `risk_alert` is a catalog event whose detector status is only the lab's claim (UNVERIFIED here); stays asset-only. DEC-EVENT-AUDIO

## 4. Open questions (no DEC id yet)

- Font: the lab records an owner choice of an open-licence font (2026-11-13) but no DEC records it, and the style decision's closure evidence still names the old system font. Owner: product owner; raised 2026-11-13.
- Studio scope and pack count are undecided; DEC-COMMERCE-INTENT records intent only. Owner: product owner; raised 2026-11-13.
- Disposal of the parked comparison copies and the taste-test copies is the owner's written decision (ledger `dispose_when`). Owner: product owner; raised 2026-11-13.

## 5. Artefacts

Machine list: `state.json`. Short form:

- Governing: `lab:AGENTS.md`, `lab:fable/DESIGN-SYSTEM.md`, `lab:BRIEF.md` (file contract).
- Code and shipped set: `lab:fable/build.mjs`, `lab:fable/src/**`, `lab:fable/*.webm|*.wav|*-sheet|*-hold|*-final`, `mapping.json`, `report.json`, `build-log.txt` (change only by owner-approved build).
- Private copies of the baseline (`lab:fable-*/`, `lab:nk-*/`): registered in the ledger registry, not described here.
- Repo side: `contracts/event-catalog.json`, `crates/kohbor-pack-author/src/lib.rs`, `docs/design/MEDIA-PACK-SPEC.md` (MEDIA-004).

## 6. Traps and lessons

- Hash every shipped WebM and WAV before and after any task; `--only` builds must leave the others untouched. `lab:AGENTS.md` section 4.
- JSON must be UTF-8 without BOM, LF; PowerShell `Set-Content` writes a BOM and Node rejects the file. `lab:AGENTS.md` section 7; `lab:fable/DESIGN-SYSTEM.md` section 11.
- After `--only` for an asset-only banner the build log text looks unchanged; compare file times and `report.json`. `lab:fable/DESIGN-SYSTEM.md` section 11.
- `--skip-pack` is needed when the pack tool is absent; the tool will not write inside a git worktree, so never aim lab output at the repo. `lab:AGENTS.md` section 5; `crates/kohbor-pack-author/src/lib.rs`.
- Changing the font changes glyph widths and descender room; the width rules in the design system were measured for the old font. `lab:AGENTS.md` section 7.
- The notice template fixes one prop and its fitted lead word can drop under the minimum size; entrance clipping and covered glyphs are inherited and unfixed. `lab:AGENTS.md` section 7.
- An automatic word check proves intelligibility only, never accent; a take is "approved" only by the owner; clip-to-text pairings from ASR similarity are not listened pairings. `lab:fable/DESIGN-SYSTEM.md` section 15; `lab:AGENTS.md` section 3.
- Compare models with one private copy each, blind, and show motion side by side, not contact sheets. `lab:AGENTS.md` sections 4, 5.
- Agents in the T3 UI default to approval-required; long render jobs go to a bounded `codex exec`. A background wait is not progress. `lab:AGENTS.md` section 7.
- The owner may be playing Dota 2: one render at a time, below-normal priority, ask the lead before any GPU job; no network, downloads or installs without the owner's permission for that file. `lab:AGENTS.md` section 4.

## 7. Who may write, how to claim, how to ask

- One writer per folder. Create a claim file in the coordination ledger before the first write (docs/domains/README.md section 4); the lead closes it. Workers never edit this card; they report what it needs.
- Do not touch git in the lab. Nothing from the lab enters the repository, a package or a release until the owner says so.
- Every lab report lists as unverified: nobody has listened to the audio; overlay playback is untested; rights are unverified (`lab:AGENTS.md` section 4, rule 8).
- Ask: read this card, then `lab:AGENTS.md`; then a read-only consultant; then the session holding the claim (in-flight facts only); the owner for taste, wording, sound, rights and every "approved" (docs/domains/README.md section 3).
