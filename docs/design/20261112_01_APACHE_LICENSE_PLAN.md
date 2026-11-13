---
title: flankr under Apache-2.0, with libherdr_term's own licence and the companion app's notices
date: 2026-11-12
status: Implemented
covers:
  - "The whole flankr project licensed Apache-2.0: a root LICENSE, a
    `license` field in every flankr manifest, and a Licence section in the
    README"
  - "libherdr_term carrying its own LICENSE and NOTICE, and a README that
    explains exactly how herdr's source is read with syn and what of it is
    compiled in"
  - "The herdr submodule moved from vendor/herdr to external/herdr"
  - "apps/companion/README.md: the app, its licence, and the licences and
    notices of the fonts, speech models and libraries it contains or loads,
    with the licence texts under apps/companion/licenses/"
  - "Not covered: a licence screen inside the app; a full generated list of
    crates.io dependencies; the licence of the owner's sibling libraries,
    which keep their own"
sources:
  - "Owner (2026-11-12): \"it would make sense to make libherdr/libherdr_term
    Apache-2, I'm still deciding on the backend license overall\"; then \"let's
    go with Apache-2.0 on the full project and a specific license on
    libherdr_term with it's own README, explain how the herdr source is used
    clearly (with syn, etc.). either link to the mobile README with font and
    voicexn licenses and notices or incorporate them in the main README, we
    should also move the herdr submodule to ./external instead of vendor.
    write a plan for the license part\""
  - "The state on 2026-11-12: no LICENSE file and no `license` field anywhere
    in flankr; GitHub reports no licence for the public tmzt/flankr"
  - ".gitmodules (vendor/herdr, https://github.com/herdrdev/herdr, shallow,
    pinned at 2713feb); vendor/herdr/LICENSE (Apache-2.0, no NOTICE file)"
  - "crates/libherdr_term/build.rs, build/extract.rs, src/lib.rs (the
    `include!` of `$OUT_DIR/herdr_wire.rs`) and README.md (\"Licence of the
    generated code\")"
  - "apps/companion/Cargo.toml (libmsdf's font features); libmsdf's README
    \"Licensing\" and FONTS-LICENSE-*.txt; libvoicexn's README \"Licenses of
    the models\" and \"License\""
related:
  - "docs/design/20261027_08_HERDR_TERMINAL_CRATE_PLAN.md (libherdr_term;
    decisions 2 and 14 name vendor/herdr, which this plan moves)"
  - "docs/HERDR_WIRE.md (the pin's log)"
---

# flankr under Apache-2.0

## Why

flankr is public on GitHub with no licence, which leaves all of it all
rights reserved. The owner has chosen Apache-2.0 for the whole project.
libherdr_term is the one crate that compiles someone else's code into
flankr: herdr's protocol types, generated from herdr's Apache-2.0 source at
build time. It gets its own licence file, notice and an exact account of
what it takes from herdr. The companion app bundles fonts under Apache-2.0
and the SIL Open Font License, and loads speech models with their own
licences. Those notices get one place, the app's own README, which the main
README links to.

The herdr checkout is not vendored code: none of it is committed (the
repository holds only the submodule's pin) and none of it is built. It moves
to `external/herdr`, which says so.

## Decisions

1. **flankr is Apache-2.0.** A root `LICENSE` holds the Apache License 2.0
   text verbatim, the same text as libmorph's `LICENSE-APACHE`. There is no
   root `NOTICE`: flankr's own code needs none, and the only notice owed to
   herdr travels with libherdr_term (decision 3).

2. **Every flankr manifest says so.** `license = "Apache-2.0"` goes into
   the `[package]` of all thirteen flankr packages: the root `flankr`
   crate, `crates/flankr-audio`, `flankr-companion`, `flankr-daemon`,
   `flankr-markdown`, `flankr-speech`, `libflankr_blob`, `libflankr_proto`,
   `libflankr_term`, `libherdr`, `libherdr_term`, `qrcodeurl`, and
   `apps/companion`. Each manifest states it literally, not through
   `license.workspace`, because `apps/` is a separate workspace.
   Code in `crates/flankr-audio/src/morph.rs` and
   `apps/companion/scripts/push-*.sh` was copied from libvoicexn, which is
   the owner's own work under MIT OR Apache-2.0, so it can be licensed
   Apache-2.0 here.

3. **libherdr_term carries its own licence.** The crate is Apache-2.0 like
   the rest, and because its build output contains herdr's code it also
   carries the files that must travel with it:
   - `crates/libherdr_term/LICENSE`: the Apache License 2.0 text;
   - `crates/libherdr_term/NOTICE`:

     ```text
     libherdr_term, part of flankr.
     Licensed under the Apache License, Version 2.0 (LICENSE).

     This crate compiles code generated from herdr
     (https://github.com/herdrdev/herdr), Copyright the herdr authors,
     licensed under the Apache License, Version 2.0. The generated code is
     herdr's own definitions with the changes listed in this crate's
     README.md, "How herdr's source is used".
     ```

     Apache-2.0 section 4(d) does not require it, since herdr has no
     `NOTICE` file. It is there so that anyone shipping the daemon
     binary finds herdr's attribution in one file.

4. **libherdr_term's README explains how herdr's source is used.** Its
   "Licence of the generated code" section is replaced by two sections,
   **How herdr's source is used** and **Licence**. **How herdr's source
   is used** states, from `build/extract.rs`:
   - **Which source.** The pinned herdr checkout, `external/herdr`, or the
     directory in `HERDR_SRC`. herdr is never a Cargo dependency, and
     the workspace excludes it, so it is never built or linked.
   - **Which files.** `build.rs` reads exactly five:
     - `src/protocol/wire.rs`;
     - `src/protocol/endpoint.rs`;
     - `src/input/model.rs`;
     - `src/api/schema/common.rs`;
     - `src/config/model.rs`.
     Each is parsed with `syn` (version 2, features `full` and
     `visit-mut`) into a syntax tree. herdr's code is never compiled as
     herdr, only read.
   - **What is kept.** From `wire.rs` and `endpoint.rs`:
     - the type definitions and constants;
     - seven named free functions (the list in `KEPT_FNS`);
     - four trait impls: `Clone` for `CellData`, and `Display`, `Error`
       and `From` for `FramingError`.
     From the other three files, the three types the wire types name:
     `WindowsKeyRecord`, `AgentStatus` and `ToastHerdrPosition`.
   - **What is removed or changed:**
     - all other `impl` blocks;
     - items and fields under `cfg` conditions that are false here,
       including herdr's tests;
     - the `bincode::Decode` and `schemars::JsonSchema` derives;
     - herdr's `crate::…` and `super::…` paths, rewritten by a `syn`
       `VisitMut` pass to the flat module's names.
     Nothing else is edited.
   - **Checks.** The build stops with one readable line if any of the 24
     items in `REQUIRED` is missing, or if `PROTOCOL_VERSION` is not an
     integer literal. herdr's source then no longer matches the extractor,
     and nothing is generated.
   - **Output.** The kept items are printed with `quote`, `proc-macro2` and
     `prettyplease` to `$OUT_DIR/herdr_wire.rs`. The file opens with a
     header naming herdr's files and licence, and `src/lib.rs` includes it
     as `libherdr_term::herdr`. Nothing generated is committed. Run
     `cargo run -p libherdr_term --example dump_herdr_types` to print it.
   - **What this means for distribution.** The compiled crate, and so the
     daemon binary, contains herdr's definitions. Distributing it means
     carrying this crate's `LICENSE` and `NOTICE`.
   The section corrects the current README's "drops herdr's `impl` blocks":
   four `impl` blocks are kept. **Licence** says the crate is Apache-2.0
   and points to `LICENSE` and `NOTICE`.

5. **The submodule moves to `external/herdr`**, at the same pin (`2713feb`)
   and still shallow. The submodule's name moves with it. In
   `.gitmodules`, `[submodule "vendor/herdr"]` becomes
   `[submodule "external/herdr"]` with `path = external/herdr`, so the name
   and the path agree. The move: `git submodule deinit -f vendor/herdr`,
   `git rm vendor/herdr`, then
   `git submodule add --depth 1 https://github.com/herdrdev/herdr external/herdr`,
   checked out at `2713feb56e9c9db33992db0a262c68f7426d46dd`, with
   `shallow = true` set in `.gitmodules`. Every reference moves:
   - the root `Cargo.toml` `exclude`;
   - `scripts/install.sh` (comment and `HERDR_SRC` default);
   - in libherdr_term: `build/extract.rs` (`herdr_root`, the error's
     `git submodule update --init external/herdr`), `build.rs`,
     `src/fake.rs`, `src/lib.rs`, `tests/extract.rs`,
     `examples/dump_herdr_types.rs`, `README.md`, and the `Cargo.toml`
     description.
   Frozen plans and earlier `CHANGES.md` entries keep saying
   `vendor/herdr`. `docs/HERDR_WIRE.md` is append-only (Rule 3.6), so it
   gets a dated section recording the move. A checkout made before the move
   runs `git submodule sync` and `git submodule update --init external/herdr`.
   It then deletes the leftover `vendor/herdr` directory and
   `.git/modules/vendor/herdr` by hand. The README's Building section says
   so.

6. **The companion app's notices live in `apps/companion/README.md`.**
   - **The README.** It describes the app and how it is built and given its
     models, says the app is Apache-2.0 as part of flankr, and has a
     **Third-party licences and notices** section. That section covers:
     - the fonts it bundles through libmsdf's features (Roboto Regular,
       Bold and Italic under Apache-2.0; Noto Sans Mono and the terminal
       symbol page under OFL-1.1), with their copyright lines;
     - the speech models it loads, with each licence and attribution as
       libvoicexn's README states them; for example, CC-BY-4.0 requires an
       app to show its attribution;
     - the owner's libraries it links: libmorph, libmobile, libvoicexn and
       libmsdf under MIT OR Apache-2.0, and libcontrols under MIT;
     - one sentence on the crates.io dependencies.
   - **The licence texts.** They are copied into `apps/companion/licenses/`
     and linked from the README: at least `OFL-1.1.txt` from libmsdf's
     `FONTS-LICENSE-OFL.txt`, and a font Apache text from
     `FONTS-LICENSE-APACHE.txt`. Each copy is word for word, with the
     source repository and file named in the README.
   - **Drafting.** A draft is written ahead of this plan's approval as an
     uncommitted file for the owner to review. It lands in step 3 as
     reviewed, together with any open items it raises that the owner has
     settled.
   - **Herdr.** The app contains no herdr code. That is checked against
     `apps/companion`'s path dependencies in step 3 and stated in the
     README.

7. **The main README links instead of incorporating.** It gets a short
   `## Licence` section, before `## Architectural decisions`:
   - flankr is Apache-2.0 (`LICENSE`);
   - libherdr_term contains code generated from herdr, see its README and
     `NOTICE`;
   - the companion app bundles fonts and loads models under their own
     licences, see `apps/companion/README.md`.

   The main README describes the daemon and the CLI, and the app's notices
   apply only to the app's build, so they are kept beside the app. One
   list kept in one place also can't drift out of step with a second copy.

8. **No version changes.** A `license` field, licence files and READMEs
   change no public API (Critical Rule 5) and nothing a user of the app or
   the daemon sees or does (FLANKR RULES 1 and 2). The submodule's new path
   changes how a source checkout is prepared, but no command, flag, config
   key or runtime behaviour. No crate, daemon or app version changes, and
   no `VERSIONS.md` or `UPDATES.md` entries are written.

## Steps

Each step lists its commits, one package or one set of repository-level
files per commit (Critical Rule 3.1). Before every code commit, the package
passes `cargo fmt -p <crate> -- --check`, `cargo clippy -p <crate>
--all-targets` and `cargo test -p <crate>`. For `apps/companion`, the same
commands run in `apps/`.

### Step 1: the submodule moves to `external/herdr`

1. libherdr_term: `herdr_root` looks for `external/herdr` first and falls
   back to `vendor/herdr`, so the build works on both sides of the move.
2. Repository: the move in decision 5, which covers `.gitmodules`, the
   gitlink, the root `Cargo.toml` `exclude` and `scripts/install.sh`. It
   also appends the `docs/HERDR_WIRE.md` section. The plan's
   `In implementation` status, its immutable marker and its README row go
   in this commit, and the `CHANGES.md` addendum that starts the plan
   goes in its own commit just before.
3. libherdr_term: the fallback is removed. Every remaining `vendor/herdr`
   in the crate becomes `external/herdr`, including the build error's fix
   line and `tests/extract.rs`'s expectation of it.

**Done when:** a fresh `git clone --recurse-submodules` builds and tests
libherdr_term, with `external/herdr` at `2713feb`. Outside `docs/design/`,
`CHANGES.md` and `docs/HERDR_WIRE.md`'s earlier sections,
`git grep vendor/herdr` finds nothing.

### Step 2: libherdr_term's licence and README

One libherdr_term commit adds `LICENSE` and `NOTICE` (decision 3) and the
README's two new sections (decision 4), with `license = "Apache-2.0"` in its
`Cargo.toml`.

**Done when:** the README's account matches `build/extract.rs`. That is,
each name it gives (the five files, `KEPT_FNS`, the four impls, the three
outside types, the two dropped derives, the 24 required items) appears in
the extractor. `NOTICE` and `LICENSE` are in the crate.

### Step 3: the companion app's README and licence texts

One `apps/companion` commit adds `README.md`, `licenses/`, and
`license = "Apache-2.0"` in its `Cargo.toml`. The README is the reviewed
draft from decision 6.

**Done when:**
- every bundled font feature in `apps/companion/Cargo.toml` and every model
  that a `scripts/push-*.sh` script copies appears in the README, with its
  licence and a link to the licence text;
- each file in `licenses/` is identical to its source;
- the README has no unresolved open item.

### Step 4: the root licence

One repository commit adds `LICENSE` (decision 1) and the README's
`## Licence` section (decision 7).

**Done when:** `LICENSE` is the Apache 2.0 text. The README's links to
`LICENSE`, `crates/libherdr_term/README.md`,
`crates/libherdr_term/NOTICE` and `apps/companion/README.md` resolve.
`## Architectural decisions` is still the README's last section.

### Step 5: `license` in the remaining manifests

One commit per package adds `license = "Apache-2.0"` to the eleven
manifests not done in steps 2 and 3: the root crate and the ten other
crates in `crates/`.

**Done when:** `cargo metadata --no-deps --format-version 1` at the root and
in `apps/` lists `Apache-2.0` as the licence of every flankr package. The
amendment that sets `status: Implemented` lists the commits for each step.

## Open questions for the owner

1. **"A specific license on libherdr_term."** This plan reads it as
   Apache-2.0 with the crate's own `LICENSE` and `NOTICE` (decision 3).
   If you meant a licence different from the rest of flankr, it would
   still have to be compatible with herdr's Apache-2.0 code, which the
   crate compiles in. Apache-2.0 alone, or "MIT OR Apache-2.0" for
   flankr's own parts of the crate, are the natural choices.
2. **A copyright line.** The plan adds none: the Apache text needs none,
   and the `NOTICE` names only the herdr authors. If you want one, for
   example in the README's Licence section and the `NOTICE`, give its
   exact wording.
3. **A licence screen in the app.** CC-BY-4.0 models may require
   attribution where the app's user sees it. This plan only documents
   that requirement. A screen in the app's settings would be a separate
   plan, with an app version bump.

## Amendment — 2026-11-12 15:55 EST: approved, with the risk section in the main README and recommended answers

The owner approved this plan on 2026-11-12 ("yes"). They also said the
section on how approvals reach an agent "needs to be in the main app, and it
should reference the license Apache-2 and its disclaimer instead", that MIT's
wording and Apache-2.0's both serve to disclaim warranty, and that they will
review the recommended changes here. Implementation has not started; the
status stays `Draft` until step 1's first commit. This amendment adds to and
replaces the earlier text as follows.

1. **A "Use at your own risk" section in the main README (new decision 9).**
   It goes before `## Licence`, and lands in step 4's commit. Its text:

   > ## Use at your own risk
   >
   > flankr is a tool for one person to run their own coding agents on
   > their own computer. It relays what you say and type to those agents,
   > from the voice session or the companion app, and relays their
   > questions and replies back; it does not judge their work.
   >
   > Approvals and answers do not go through any interface of the agents'
   > own. The daemon types them into the agent's terminal pane as keyboard
   > input, through herdr, and it decides where that input goes
   > heuristically: it recognises the agent's prompt or question from the
   > text on screen, and picks the keys to send from what it recognised. If
   > the screen is not what the daemon took it to be, for example because
   > the agent changed its layout or the screen changed while the keys were
   > in flight, the keys can land somewhere other than intended. Approving
   > or answering through flankr has the same effect as typing at the
   > keyboard, so what the agents then do is yours to review.
   >
   > flankr is licensed under the Apache License, Version 2.0, and is
   > provided on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
   > KIND, as section 7 (Disclaimer of Warranty) of the licence states.
   > Section 8 (Limitation of Liability) limits the contributors' liability
   > for damages arising from its use, which includes what an agent does
   > with input flankr sent it. See [LICENSE](LICENSE) for the full terms.

   The app's README has no disclaimer of its own (this replaces the draft's
   "For personal use" section, which quoted MIT's warranty text). It has a
   short **Use at your own risk** section that says the app's approvals
   reach agents as the daemon's keyboard input and links to
   `../../README.md#use-at-your-own-risk`. Step 4's **Done when:** gains:
   the section is present, before `## Licence`, and its link to `LICENSE`
   resolves. Step 3's gains: the app's README links to it.

2. **Licence text file names (replaces decision 6's `OFL-1.1.txt` and "a
   font Apache text").** The copies are
   `apps/companion/licenses/FONTS-LICENSE-APACHE.txt` and
   `apps/companion/licenses/FONTS-LICENSE-OFL.txt`, named as libmsdf names
   them, each a byte-for-byte copy that keeps libmsdf's per-font copyright
   notices above the licence text.

3. **Recommended answers to the open questions, for the owner's review:**
   1. libherdr_term: Apache-2.0 with its own `LICENSE` and `NOTICE`, as
      decision 3 has it. No different licence.
   2. No copyright line beyond what the `NOTICE` says of herdr.
   3. No licence screen in this plan. The CC-BY-4.0 credit Kyutai's model
      requires goes in the app's README now, and an in-app screen (with an
      app version bump) is a later plan.

4. **Recommended handling of the draft README's open items (step 3):**
   1. The STT model's `tokenizer.model` and `config.json`: confirmed from
      the licence stated in their Hugging Face repository
      (`kyutai/stt-1b-en_fr-candle`) before step 3's commit. If it is
      stated for the repository as a whole, the README says that.
   2. Kyutai's attribution: worded as the model card asks. Where it asks
      nothing specific, the README uses the CC-BY-4.0 form: the model's
      name, "by Kyutai", a link to the model, and "CC BY 4.0" with a link
      to the licence.
   3. No full crates.io licence list in this plan: the README keeps its
      one-sentence summary and the `cargo about` suggestion. A generated
      list, which would keep rqrr's ISC notice, is a later plan.
   4. The launcher icon's line ("from a source image made with Gemini")
      stays: it records where the icon came from, which bears on its
      licence.

## Amendment — 2026-11-13 01:33 EST: the owner's text rules, the merge, and implementation started

The owner, on 2026-11-12, after the 15:55 EST amendment: "uee a subagent
and Claude worktree to prepate proposed readme and l8cense changes. be
clear thst voice models are downloaded separately and include the model
card references, b7t don't imply they're shipped in the codebase. dint
mention other projects by name or reuse the exact language except in
reference to the voice models. the long disclaimer and detailed
explanation should be shortened and clear to users. dont merge any of
this until explicitly approved." Then, on 2026-11-13: "merge and push the
licence branch". This replaces, where they disagree, decision 6 and the
15:55 EST amendment's item 1:

1. The main README's **Use at your own risk** is one short paragraph:
   keyboard input through herdr, placed heuristically, the same as typing
   it yourself, Apache-2.0 "AS IS" with sections 7 and 8 of LICENSE. The
   quoted text of the 15:55 amendment is not used.
2. The companion app's README says the voice models are downloaded
   separately and are not in the repository or the APK, links each
   model card (Kyutai's STT, CC-BY-4.0 for the weights; Gradium's Phonon
   and its voices, Apache-2.0), and names no other project, nor where
   any code, script or text came from; dependencies appear only as rows
   of its licence table. The font licence copies keep libmsdf's file
   names but the README does not name the library they came from.
3. The steps were built out of order on a branch made for review and
   merged on the owner's instruction (merge commit 6f145ad): step 2
   (libherdr_term, 862e6ca), step 3 (the app, e23b6c6), step 4 (the root
   LICENSE and README sections, c4a5f65), step 5 (the license fields,
   1309488 to 9601b0d). Step 1, the move of the submodule to
   external/herdr, is not done; libherdr_term's README describes
   vendor/herdr until it is.
4. Open, from the review: the STT repository states a licence only for
   its weights, not its tokenizer and config; the quantized model file
   the app loads is not published, and how users get it is undecided; the
   font licence copies come from a newer libmsdf than the app pins; two
   origin comments remain in apps/companion/Cargo.toml and the push
   scripts.

## Amendment — 2026-11-13 01:33 EST: implementation started; this document is now immutable

Steps 2 to 5 are merged (above). From here the plan changes only on the
owner's explicit instruction.

## Amendment — 2026-11-13 01:41 EST: implemented

Every step is done and its **Done when:** holds, so the status is
`Implemented`. Step 1 was done on the owner's instruction of 2026-11-13:
"move the herdr submodule to external and complete the other steps before
fully merging". Its departures, among them where `vendor/herdr` is still
named as a record of the past, are in `CHANGES.md`'s entry of this day
for step 1. The commits for each step:

1. **Step 1**, the submodule moves to `external/herdr`:
   - 3a9a482: libherdr_term looks in `external/herdr` first, falling
     back to `vendor/herdr`;
   - c805432: the move (`.gitmodules`, the gitlink at `2713feb`, the
     root `Cargo.toml` `exclude`, `scripts/install.sh`, and
     `docs/HERDR_WIRE.md`'s section);
   - 3069d13: libherdr_term's fallback removed and every reference moved;
   - 229bfd1: libherdr's doc comment moved.
   A fresh `git clone --recurse-submodules` checks out `external/herdr`
   at `2713feb` and passes `cargo test -p libherdr_term`.
2. **Step 2**, libherdr_term's licence and README: 862e6ca.
3. **Step 3**, the companion app's README and licence texts: e23b6c6,
   and 3b90c72, which makes the font licence copies byte-for-byte copies
   from the libmsdf revision the app pins.
4. **Step 4**, the root licence: c4a5f65.
5. **Step 5**, `license` in the remaining manifests: 1309488 to 9601b0d.

Steps 2 to 5 were merged in 6f145ad.

These items, from the 01:33 EST amendment, remain open for the owner.
They are documented, not settled, and none of them is a step of this plan:

- the STT repository states a licence only for its weights, not for
  `tokenizer.model` and `config.json`;
- the quantized model file the app loads, `model.q4k.gguf`, is not
  published, and how users get it is undecided;
- two origin comments remain, in `apps/companion/Cargo.toml` and the
  push scripts.
