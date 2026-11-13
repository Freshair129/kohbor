---
title: "Package Format Review"
doc_id: "PACKAGE-FORMAT-REVIEW"
version: "0.1.0"
status: "draft"
updated: "2026-11-12"
owner: "Product owner"
approval_status: "not_approved"
---

# Package Format Review

Independent agent review (product, security, build and maintenance cost) of the owner's challenge: *why does a media package need its own file type at all, when a folder of WAV files mapped by ID would do?* This is an agent review, not a human, security or legal review. It decides the first-release rules in section 9; the owner confirms the points in section 11.

**Evidence labels.** `[M]` MEASURED this session (method in section 2). `[R]` FROM-REPO (file and place given). `[V]` VERIFIED on the web (URL, date). `[U]` UNVERIFIED.

## 1. Short answer to the owner

1. **Size: a small gain. Speed: none.** Deflate saves 20 to 30% on the announcer WAV clips, about 3 to 4 MiB per pack `[M]`. WebM gains nothing (it is Stored) `[R]` MP-03. Playback never reads the container: an installed revision is already a plain folder, and audio is decoded to PCM in memory at activation `[R]` `store.rs` header comment, S3-5 section 3, MP-08.
2. **So the app already works the way the owner pictures it.** Inside Kohbor's data directory a pack is a folder holding `descriptor.json` (the ID-to-file map) and the media files `[R]` `store.rs` line 5.
3. **What one file buys that a folder cannot:** it can be attached in Discord or email and downloaded in a browser as one item; it has one digest for the idempotent install request; it cannot be half-copied. These are transport benefits only.
4. **A custom extension does not compress, speed up or protect anything.** Its only technical value is to tell Windows and the user "this is not an ordinary ZIP" and, later, to open the app on a double click.
5. **Kohbor has no own extension today.** MP-03 says "indexed ZIP" and names no extension; the authoring tool writes `<package_id>-r<revision>.zip` `[R]` `kohbor-pack-author/src/lib.rs` line 456. `.gann` belongs to G-Maiden `[R]` `gann-package-v1-local-authoring.md` line 16.
6. **The current combination is the worst one:** the file is named `.zip`, but no ordinary ZIP tool can produce a file the reader accepts (7 of 7 tool outputs rejected, section 4) `[M]`.
7. **The format is not copy protection** and was never specified as one (section 7).

## 2. Measurements

Method `[M]`: a Python script (run with `python -I`, scripts and outputs kept only in the session scratchpad and deleted afterwards) read each WAV into memory and applied raw Deflate with `zlib.compressobj(6, DEFLATED, -15)`. Nothing was copied, played or uploaded. All `.wav` files under each folder were included (pack clips, unmapped clips and one long preview file).

| Folder (under `D:\User\Downloads\`) | WAV files | Format | Raw | Deflate 6 | Saving | Per-file ratio |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| `VRZO-ANNOUNCER-pack-20261029-v0.2.0` | 37 | PCM16 mono 44.1 kHz | 27.53 MiB | 19.92 MiB | 27.6% | 0.476 to 0.819 |
| `NAKOM-HON-ANNOUNCER-pack-20261028` | 40 | PCM16 mono 44.1 kHz | 22.15 MiB | 17.74 MiB | 19.9% | 0.693 to 0.917 |
| `VRZO-ANNOUNCER-pack-20261028` | 37 | PCM16 mono 44.1 kHz | 16.87 MiB | 14.45 MiB | 14.3% | 0.813 to 0.931 |

Simulated authoring profile `[M]`: each clip cut to 15 s and resampled to mono 48 kHz PCM16 by linear interpolation in the script. This approximates the tool's own resampler (`audio.rs`), which was not run.

| Folder | Clips | Duration (min / median / max) | Over 15 s | Converted raw | Deflate 6 | Saving | Per-file ratio |
| --- | ---: | --- | ---: | ---: | ---: | ---: | --- |
| VRZO v0.2.0 | 37 | 2.01 / 3.26 / 115.51 s | 2 | 13.32 MiB | 9.29 MiB | 30.2% | 0.474 to 0.817 |
| NAKOM | 40 | 1.48 / 3.00 / 120.70 s | 2 | 13.39 MiB | 10.71 MiB | 20.0% | 0.687 to 0.916 |

Reading: a whole announcer pack is about 13 MiB of audio as a folder and 9 to 11 MiB in the container. The largest converted clip is 1.44 MiB against a 32 MiB payload limit. Size is not a reason for or against either form.

Other measured facts: neither named folder contains a `.webm` file (extension counts: VRZO v0.2.0 has json, csv, sha256, html, wav, md, zip, py, ps1; NAKOM adds txt, mp4, m4a, jpg, png, log) `[M]`. See section 10, finding F1.

## 3. What each form buys and costs (questions 1 and 2)

| Topic | One file (container) | Folder |
| --- | --- | --- |
| Size | 20 to 30% smaller on WAV `[M]`; 0% on WebM `[R]` | Baseline |
| Load and playback | No difference: installed form is a folder, PCM is prewarmed `[R]` | Same |
| Integrity | Per-file SHA-256 and size in the descriptor do the real work `[R]` `container.rs` 1208 to 1213. The container digest adds a transport check | Identical per-file checks; no single transport digest |
| Package identity | The registry already stores `descriptor_sha256` and re-checks it at resolve `[R]` `store.rs` 522, 611, 923. The descriptor lists every payload's size and SHA-256, so its hash commits to the whole content | Same hash works; it does not depend on any container |
| Atomic copy and transfer | One item: complete or absent | A tree can be half-copied or changed during the copy |
| Sharing with non-technical users | Attach or download one file | A folder cannot be attached or downloaded as such; people zip it by hand, which produces the files measured in section 4 `[U]` (chat and mail client behaviour not tested) |
| Signing later (DEC-PACK-CRYPTO) | Sign a statement binding `package_id`, revision and `descriptor_sha256`; it may also bind container digest and size | The same statement minus the container fields. Signing does not require a container |
| Accidental modification | Opaque file is rarely edited | Easy to edit a file; the digest check then rejects it (correct, but a support case) |
| Installer idempotency (MP-06) | `request_id` + `expected_container_sha256` `[R]` `store.rs` 410 to 470 | Needs a contract change to `expected_descriptor_sha256` |
| Reader attack surface | Own ZIP reader; a narrow accepted language is the mitigation | No ZIP parser, but new surface: symlinks and junctions, long paths, reserved names, case collisions, unbounded trees, files changing during the walk |

**Reuse if a folder were the source.** Unchanged: `descriptor.rs` (581 lines), `media.rs` (671), `trust.rs`, `limits.rs`, `fsutil.rs` (reparse-point, device-name and case-key helpers already exist), the store's commit, registry, activation and resolve logic, and in `container.rs` the shared half: bounded copy with SHA-256, staging file creation, path validation, inventory and media checks (about 665 of 1,252 lines by my reading of the function boundaries `[R]`). TOCTOU is already solved by the existing pattern: copy into private staging first, verify the copy `[R]` `store.rs` 445.

**Deleted if ZIP were removed:** about 585 ZIP-specific lines in `container.rs` (end record search, central directory, local headers, profile checks, entry extraction), `kohbor-pack-author/src/archive.rs` (138 lines), the test ZIP builder in `tests/common/mod.rs` (393 lines, partly reused for fixtures) and the structure cases among the 28 tests in `tests/hostile.rs` (563 lines) `[R]` line counts by `wc -l`. **Added:** a bounded tree walker with reparse-point refusal, entry and byte caps and a staged copy, an estimated 250 to 350 lines plus about 12 hostile tests (junction, symlink, device name, case collision, oversize tree, file changed during copy) and a change to the MP-06 request contract `[U]` (estimate, not built).

Net effect of folder-only: roughly 700 lines removed, 300 added, one contract changed, and one-file transfer lost. A future Store download would need a one-file form again.

## 4. Do ordinary ZIP tools produce an acceptable package? (question 3)

Method `[M]`: a four-file test folder (`descriptor.json`, `audio/alpha.wav`, `audio/zeta.wav`, `video/clip.webm`) was zipped in the scratchpad with each tool on this machine (Windows 11 26300, PowerShell 5.1.26100). The central and local headers were parsed with a script. The verdict column applies the rejection rules in the order of `stage_package_inner` `[R]` `container.rs` 1102 to 1137 and the doc comment at 1036 to 1054. The Rust reader itself was not run (no cargo in this review), so the verdict is an inference from measured bytes and read code.

| Producer | Entry order | Names | Timestamp | External attributes | Extras, flags | Directory entries | First rejection |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Explorer zipped folder, folder dropped in | folder order | `mypack/...` prefix | real time | `0x10` / `0x20` | none | 2 | `DescriptorNotFirst` |
| Explorer zipped folder, contents dropped in | `audio/...`, then descriptor | `/` | real time | `0x20` | none | 0 | `DescriptorNotFirst` |
| `Compress-Archive -Path folder\*` | payloads, descriptor last | **backslash** `audio\alpha.wav` | real time | `0` | none | 0 | `DescriptorNotFirst` |
| `Compress-Archive -Path folder` | descriptor first | `mypack\...` backslash | real time | `0` | none | 0 | `DescriptorNotFirst` (name has prefix) |
| .NET `ZipFile.CreateFromDirectory` | descriptor first | backslash | real time | `0` | none | 0 | `HeaderMismatch` (timestamp) |
| Windows `tar.exe -a` (bsdtar 3.8.9) | sorted, dirs included | `/` | real time | Unix mode | extras `0x5455`, `0x7875`; data-descriptor flag; local CRC 0 | 2 | `DescriptorNotFirst` |
| Python `zipfile` | descriptor first | `/` | real time | `0100644 << 16` | none | 0 | `HeaderMismatch` (timestamp) |

The two ZIP files already present in the owner's clip folders (22 and 13 entries; headers only were read) fail the same way `[M]`.

**Result: 7 of 7 ordinary producers are rejected.** Only `kohbor-pack-build` can write a package. That alone costs the community plan little, because a creator needs the tool in every option: the descriptor carries a size and SHA-256 per file, the audio must be mono 48 kHz PCM16 of at most 15 s, and every binding needs a silent WebM `[R]` `media-manifest.schema.json` required list, EVENT-AUDIO-SPEC section 6. Nobody writes that by hand. The real cost is narrower: a correct package that someone extracts and zips again stops installing, with a reason that means nothing to a player.

Which strict rules are needed for safety, and which only for byte-identical builds `[R]` rule list, my classification:

| Needed for safety | Only for a reproducible container |
| --- | --- |
| Container, per-entry, aggregate and count caps; bounded streaming inflate | Descriptor is the first entry |
| Path rules (traversal, absolute, device names, trailing dot or space, colon, case-insensitive duplicates) | ASCII-sorted payload order |
| No encryption; methods 0 and 8 only; no ZIP64, no multi-disk | Timestamp fixed at 1980-01-01 |
| Local and central header agreement; CRC check | Mode 0644 in external attributes |
| Undeclared payload, dangling reference, size and digest mismatch | No extra fields, no data descriptors, no directory entries |
| Media profile checks | No comment, prefix, suffix or gaps (also keeps one file from being two things at once) |

The right-hand column exists so the container digest is stable and signable. Since `descriptor_sha256` already identifies the content, those rules are not required for identity. They are still worth keeping for a different reason: a reader that accepts one narrow byte layout has less parser behaviour to get wrong than one that tolerates data descriptors, extras and name normalisation.

## 5. File extension (question 4)

| | Own extension | `.zip` | Folder via dialog or drag-and-drop |
| --- | --- | --- | --- |
| Tells the user not to unpack or repack | Yes | No; Explorer opens it as a folder and offers "Extract all" | Not applicable |
| Double click installs | Possible with a file association. Tauri's bundler has `bundle.fileAssociations` `[V]` https://v2.tauri.app/reference/config/ (read 2026-11-12) | No (Explorer owns `.zip`) | No |
| Open-dialog and drop filtering | Exact | Every ZIP on the disk matches | Any folder matches |
| Costs | One identity field, one installer entry, launch-argument handling in the single-instance guard, a confirm screen. None of it is built `[R]` (no `fileAssociations` in `tauri.conf.json`) | None | Folder picker, plus section 3 surface |
| Trust confusion | Must not be `.gann`: same profile, different trust roots, no compatibility promised `[R]` AGENTS.md Identity, MP-09 | Low | Low |

An extension is never a security control: the reader verifies content, not the name `[R]` MP-04 ("a plausible file extension or container header is insufficient"). Install frequency is low (a player installs a handful of announcer packs, ever `[U]`), so the file association is a convenience to add with the install UI, not a first-release need.

## 6. Options (question 5)

- **A. Strict profile as built.** A1 keeps the `.zip` name; A2 gives the file Kohbor's own extension.
- **B. Folder only.**
- **C. Folder or any standard ZIP as install sources**, one verification pipeline, a relaxed reader that keeps only the safety column of section 4, identity moved to `descriptor_sha256`.

| | A2 (decided) | B | C |
| --- | --- | --- | --- |
| Creator journey | Run the tool on clips and a mapping; send one file | Run the tool (the descriptor still needs it); zip the folder by hand to send | Run the tool; send the file, or any re-zip of it |
| Player journey | Receive one file; choose it in the app; later double click | Receive a ZIP; extract; choose the folder | Receive anything; choose file or folder |
| Security surface | Narrow reader, already tested against hostile input | No ZIP parser; new tree-walk surface | Widest: tolerant ZIP parsing **and** tree walk |
| Code and tests | No reader change. One line in the tool, one identity field, host wording | About 700 lines out, 300 in, about 12 new hostile tests | Nothing deleted; an estimated 150 to 200 lines changed in the reader (data descriptors, extras, directory entries, prefix stripping, separator normalisation), 250 to 350 added for folders, about 25 new tests `[U]` |
| Spec text | One sentence added to MP-03; none removed | Rewrite MP-03 paragraph 2 and its limits table rows for the container; change MP-06 "expected container digest"; MEDIA-006 text "archive limits" | Rewrite MP-03 paragraph 2 and the "unexpected extras/comments" clause of paragraph 4; MP-06 as in B |
| Built code | All kept | Reader half, writer and structure tests discarded | All kept, reader widened |
| Authoring tool | Output name only | Emits a folder; `archive.rs` removed | Unchanged |
| G-Maiden Studio export | Not accepted today and not promised (trust roots differ) `[R]` MP-09. A future importer stays possible because the byte profile is the same | Needs a conversion step | Structure would parse; trust still differs |
| Future signed Store | One file to host, hash and sign | Needs a one-file form again | Works |
| Sprint 3 in flight | None: `media_inspect` and `media_install` take a file path `[R]` S3-5 section 4 | Changes both commands and the request schema mid-sprint | Changes the store's identity rule (`RevisionConflict` compares container digests `[R]` `store.rs` 400 to 406) mid-sprint |

**D considered: let the app itself import a folder of clips** (run the authoring pipeline inside the host, then install the result through the same pipeline). This is the feature that would actually deliver "just a folder of WAVs mapped by ID" to a creator. It needs no reader or format change, but it is a product-scope decision: the baseline treats authoring as an external producer `[R]` MEDIA-PACK-SPEC preamble. It is listed for the owner in section 11, not decided here.

## 7. Encryption and copy protection (question 7)

**(a) Today's format gives no copy protection. Confirmed.**
- MP-03 requires the reader to reject encryption `[R]`; the reader rejects general-purpose flag bits 0, 6 and 13 as `EncryptedEntry` `[R]` `container.rs` 629.
- Installed packs are ordinary WAV and WebM files under `revisions/<install_id>/` in the user's own data directory `[R]` `store.rs` line 5; the folder is `%APPDATA%\io.github.freshair129.kohbor` `[R]` `contracts/product-identity.json`.
- G-Maiden's `.gann` is described as a plain indexed ZIP whose verifier rejects encryption; its trust mechanism is a detached Ed25519 signature over a release statement, not secrecy `[R]` `gann-package-v1-local-authoring.md` lines 16 to 18 and 22 to 26. That document also says no production signer, keys or Store backend exist.
- Anyone can rename either product's package to `.zip` and open it.

**(b) What the hash and ID do.** Per-file SHA-256: detect corruption or tampering at install and again at resolve. Container digest: bind an install request to exact bytes and make retries idempotent. `package_id` + revision: name a pack and order its versions. Signature (not built; DEC-PACK-CRYPTO): prove who published those hashes. Purchase entitlement: a separate authority that says this account may use that package ID; a valid hash proves no purchase `[R]` MP-09, SRS MEDIA-008. **None of them hides content.** A hash is computed from content anyone can read.

**(c) Is real anti-extraction achievable? No. The best case stops casual file copying only.**
- The app must turn the audio into PCM and play it on the user's machine. Windows lets any ordinary program record what an output device plays: "a client of WASAPI can capture the audio stream" of a rendering endpoint `[V]` https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording (read 2026-11-12). The same page ties any protection to trusted drivers and protected content, which an ordinary application's audio is not.
- Decryption needs a key on the client. The local core must run with no account and no network `[R]` SRS CORE-001, so a key must be stored or derivable locally, where the user (who owns the machine) can read it from disk or process memory.
- An online licence check contradicts neither fact above, and no licence server exists; hosting, accounts and legal basis are open `[R]` DEC-PREMIUM, DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL in the Decision Log.
- Revocation can stop future key delivery; it cannot recall audio already decrypted or recorded.
- How comparable products protect voice packs: `[U]` not researched to a citable standard in this review.

**(d) Options for paid or Store packs** (ordinary local and community packs stay unencrypted in all three):

| | P1: signature + entitlement, no encryption | P2: per-pack encrypted payloads, key released after entitlement, cached for offline use | P3: terms only |
| --- | --- | --- | --- |
| Stops | Using a pack as "Store-authorised" without a purchase, inside Kohbor | The same, plus copying files out of the data folder by a casual user | Nothing technical |
| Does not stop | Copying the files | Recording the output; reading the cached key; anyone who shares the decrypted audio once | Anything |
| Cost | The DEC-PACK-CRYPTO work already required for any signed pack | P1 plus: key service and hosting, key wrapping per device or account, offline grace policy, revocation, encrypted installed form, decrypt during prewarm and in the video range server, an encrypt step in the Store pipeline, key-loss support `[U]` (weeks of work and a running service; not estimated in detail) | A licence text |
| Changes | `trust.rs`, store record, host status | Reader inventory rules (hash of ciphertext or plaintext), descriptor schema (key ID, cipher), store, host prewarm and media server, authoring or Store tool, MP-03, MP-08, EVENT-AUDIO-SPEC 6 and 7 | None |
| Depends on | DEC-PACK-CRYPTO | DEC-PACK-CRYPTO, DEC-PREMIUM, DEC-ACCOUNT-ORIGIN, DEC-PRIVACY-LEGAL | DEC-PRIVACY-LEGAL |
| Honest user impact | None | A paying user can be locked out offline or after reinstall; a non-paying user loses nothing | None |

No technology settles who holds the rights to the owner's existing announcer clips. Encryption cannot create a licence, and distributing them needs recorded permission `[R]` MP-10; their rights status is `[U]` here.

**(e) Interaction with folder versus ZIP.** Payload encryption is independent of the container: encrypted blobs can sit in a folder or in a ZIP. If protection were required, these change: the installed form (ciphertext at rest instead of plain WAV), what the inventory hashes cover, a key reference in the descriptor, decryption in prewarm and in the visual server. These do **not** change: folder versus one file, the extension, strict versus tolerant ZIP rules, the install transaction. ZIP's own encryption is not a candidate; the baseline rejects it and should keep doing so.

**Recommended default.** First release: no encryption anywhere; every pack installs as `local_import` (already the only grantable authority `[R]` EVENT-AUDIO-SPEC 7). For a later Store: P1 with P3. Choose P2 only if a rights holder demands at-rest protection as a contract condition, and present it to them as a deterrent against casual copying, never as prevention.

## 8. Where a format decision is maintained (question 6)

| Maintenance point | Today | A2 | B | C |
| --- | --- | --- | --- | --- |
| MP-03 profile text, EVENT-AUDIO-SPEC 7 | yes | +1 sentence | rewritten | rewritten |
| `media-manifest.schema.json` (descriptor) | yes | same | same | same |
| `media-install-request` / `-result` schemas | yes | same | changed | changed |
| Reader `container.rs` | 1,252 lines | same | about 670 + walker | about 1,450 + walker |
| Writer `archive.rs` | 138 lines | same | removed | same |
| Hostile and crash tests | yes | same | ZIP cases replaced by tree cases | both sets |
| Authoring tool output | `.zip` name | new extension | folder | same |
| Product identity manifest | no extension field | +1 field | none | none |
| UI wording for rejections, picker filter | not built | one filter, one "repacked file" message | folder picker | both |
| File association | not built | later, optional | none | none |
| G-Maiden's own reader and writer of the same profile | separate product | unchanged | unchanged | unchanged |

A2 adds two small points (identity field, extension in the tool) and removes the mismatch between the name and the behaviour. B has the shortest list but gives up one-file transfer. C has the longest list.

## 9. Decision

The evidence is close between A2 and C. I choose **A2**. The reason is forward cost and risk, not the work already spent: the strict reader costs nothing more to keep, a tolerant reader is where ZIP parsing defects typically live, and tolerance would only help the one case of a correct package being zipped again. Folder-only removes code but removes the single-file hand-off that sharing and any future Store need.

First-release rules:

1. **Package model.** A package is `descriptor.json` (manifest 1.2.0) plus the payload files it lists. Its installed form is a plain folder, as built.
2. **Install source.** Exactly one package file in container profile 1 as built (MP-03 strict rules unchanged). No folder source and no general ZIP source in the first release.
3. **Extension.** The package file stops being named `.zip`. It gets Kohbor's own extension, recorded in `contracts/product-identity.json`. Proposed value `.kohborpack`; the owner confirms the spelling. It must not be `.gann`.
4. **File association.** Not in the first release. When the install UI is built, a double click may open the app's confirm-install screen; it never installs or activates silently.
5. **Identity.** `(package_id, revision, descriptor_sha256)` is the content identity for display, trust and any later signature statement. `container_sha256` stays as the transport and idempotency check of MP-06. No store or contract change is needed for this now; it is a direction for DEC-PACK-CRYPTO.
6. **Rejection wording.** When a file fails the layout rules that ordinary ZIP tools break (`DescriptorNotFirst`, timestamp or attribute `HeaderMismatch`, `UnexpectedExtraField`, directory entries), the UI says the file was repacked and asks for the creator's original file. Reason codes in contracts do not change.
7. **Protection.** No encryption in the first release. The Store default is signature plus entitlement (section 7).

**What the owner changes in the baseline.** MP-03 paragraph 2 is an inherited baseline sentence and stays as written. Add one sentence after it: *"The package file carries the product's own extension recorded in the product identity manifest. It is not a general-purpose ZIP: output of ordinary ZIP tools is rejected, and creators produce packages with the authoring tool."* No SRS requirement text changes (MEDIA-004, -006, -007, -008 are all still met as worded).

**Cost to keep versus change.** A2: about half a day of worker time plus document control. Keeping A1 unchanged: zero now, and a recurring support case each time someone re-zips a pack. Moving to C: an estimated 4 to 6 worker days plus a security re-review of the reader, and an identity change in the store during Sprint 3 `[U]`. Moving to B: an estimated 3 to 5 days, two contract changes, and a later return to a one-file form for the Store `[U]`.

**What would change my mind.**
- Toward C: the owner wants packages built by tools other than Kohbor's (a web tool, G-Maiden Studio output accepted as-is, or re-zipping as a supported workflow).
- Toward B: packs become audio-only with a hand-writable map and no per-file hashes. That gives up install-time integrity and any signing, so it is a larger baseline change than the container.
- Toward revisiting A2's strictness: more than occasional "repacked file" rejections once real creators share packs.

## 10. Findings outside the question

- **F1 (blocks the owner's own packs more than the container does).** Every binding requires a silent WebM `[R]` MP-03 paragraph 1, manifest `required` list; the authoring tool skips a mapping with no visual ("visual path is missing") `[R]` `kohbor-pack-author/src/lib.rs` 175 to 181. The owner's two clip folders contain no WebM `[M]`. As specified, those clips cannot become a package without a video per event. Whether audio-only bindings are allowed is a baseline decision for the owner; this review does not decide it.
- **F2.** Two clips in each folder exceed 15 s `[M]` (the long preview and one other); MP-04 caps audio at 15,000 ms. Whether the tool rejects or truncates them was not run `[U]`.
- **F3.** In the manual UI flow the "expected container digest" is computed by the host from the same file just before installing (`media_inspect` then `media_install`) `[R]` S3-5 section 4. It still protects against the file changing between the two calls, but it is not an independent trust statement; that only arrives with a signed release statement.

## 11. Migration plan and owner decisions

No change to the Sprint 3 critical path (S3-5 host commands, store, engine).

| # | Task | Owner of the change | Affected | Tests |
| --- | --- | --- | --- | --- |
| 1 | Record DEC-PACK-FORMAT (this decision) in `contracts/decisions.json` and DECISION-LOG; register this document in DOCUMENT-CONTROL | Integration owner (lead) | governance only | spec validator |
| 2 | Add `package_extension` to `contracts/product-identity.json` after the owner confirms the spelling | Integration owner | identity manifest, its example | spec validator |
| 3 | Add the MP-03 sentence; add the identity and extension bullets to EVENT-AUDIO-SPEC 7 | Lead | two documents | none |
| 4 | Authoring tool writes `<package_id>-r<revision><extension>` (`lib.rs` 456) | Worker | `kohbor-pack-author`, its 16 tests where they assert the name | update name assertions; add one |
| 5 | After S3-5: host maps the layout rejections to the "repacked file" message; the install UI's Open dialog filters on the extension | Worker | host, renderer wording | one host test for the mapping |
| 6 | With the install UI or later: `bundle.fileAssociations`, launch-argument hand-off through the single-instance guard, confirm screen | Worker | installer config, host | native Windows test (separate evidence class) |

Tests to keep unchanged: all of `crates/kohbor-pack/tests` (hostile, crash, install, activation, resolve, descriptor, media, trust, units, review_fixes, windows_fs). None is invalidated.

The owner decides:

1. The extension spelling (proposal `.kohborpack`).
2. Whether audio-only bindings are allowed (finding F1). Until then the owner's clips need a silent WebM per event.
3. Whether importing a folder of clips inside the app (option D) is in scope, and for which milestone.
4. For paid packs: P1 (recommended), P2 or P3, before DEC-PACK-CRYPTO closes.
5. Rights for the existing announcer clips: a legal matter that no format choice resolves.
