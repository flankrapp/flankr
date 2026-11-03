---
title: The Gemini key in the Keychain or flankr's config folder, never the working folder
date: 2026-11-02
status: Draft
covers:
  - "Where the daemon finds the Gemini key: the macOS Keychain first, then a
    file in flankr's config folder, then an explicit `api_key_file`; no
    longer `.private/gemini_api_key` against the working folder, nor `.env`"
  - "`flankr key set|import|status gemini`, which store and report the key
    without it reaching a command line, a shell history or a log"
  - "What the daemon logs about the key's source"
  - "Not covered: a Linux Secret Service store; the companion app calling
    Gemini directly (a separate plan, as RoundTwo does)"
sources:
  - "Owner (2026-11-02, verbatim): \"it shouldn't be reading from .private in
    an arbitrary folder like HOME, but there isn't really a good way to do
    that. also, it should be reading from a defined config directly for
    .flankr where the gemini key is defined, or we should move it into the
    keychain\"; then \"plan and implement. this could also let us implement
    direct gemini access from the app like RoundTwo (separate plan)\""
  - "The MacBook's daemon, started by `flankr pair` from the home folder on
    2026-11-02: \"[llm.backends.gemini] has no key: neither
    /Users/tmeade/.private/gemini_api_key nor GEMINI_API_KEY is set\""
  - "Apple's Keychain Services (generic passwords: service, account) and
    security(1)"
related:
  - "docs/design/20261027_02_INFERENCE_CONFIG_PLAN.md (backends, the
    discovered `gemini` backend, `api_key_file`)"
  - "docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md (decision 3: the
    background daemon is never given the key's environment variable)"
---

# The Gemini key in the Keychain or flankr's config folder

## 1. Goal

The daemon reads its Gemini key from `.private/gemini_api_key` against the
folder it was started in, or from `GEMINI_API_KEY`. Started from another
folder, as `flankr pair` did on the MacBook from the home folder, it finds
none; started in the background it is never given the variable. A key
should come from one known place that does not depend on the working
folder: the Keychain on macOS, or flankr's config folder.

## 2. Findings (read 2026-11-02)

1. `src/llm/gemini.rs`: `KEY_FILE = ".private/gemini_api_key"`, `KEY_ENV =
   "GEMINI_API_KEY"`. `src/llm/mod.rs` resolves a backend's key from its
   `api_key_file`, else the kind's default file (relative, so against the
   working folder), else the environment; `read_key_file` reads a file.
2. `src/main.rs` loads `.env` from the working folder for foreground
   commands (`dotenvy`).
3. flankr's config folder is `~/.config/flankr/` (its socket and
   `config.toml` live there).
4. A process in the user's GUI launchd session (the background daemon of
   plan 20261102_01) can read the login keychain. Over SSH before the user
   has logged in at the screen, the login keychain may be locked.

## 3. Decisions

1. **The order the daemon looks in**, for the `gemini` kind, first found
   wins:
   1. the backend's `api_key_file`, when set: an absolute path, or one
      relative to the folder of the config file that names it (never the
      working folder);
   2. **the macOS Keychain**: a generic password with service `dev.flankr`
      and account `gemini`, in the default keychain search list;
   3. **flankr's config folder**: `~/.config/flankr/secrets/gemini_api_key`
      (`$XDG_CONFIG_HOME/flankr/secrets/…` when set);
   4. `GEMINI_API_KEY`, only when it reaches the process (a foreground
      run).
   The working folder's `.private/gemini_api_key` is no longer read, and
   `.env` is no longer loaded for the key.
2. **Permissions.** The `secrets` folder is created 0700 and the file
   0600, as they are made; a key file readable by others (group or world
   bits set) is refused with a warning naming the path and the fix
   (`chmod 600`), not read.
3. **`flankr key set gemini`**: reads the key from the terminal with echo
   off (or one line from stdin when it is not a terminal), trims it,
   refuses an empty one, and stores it: in the Keychain on macOS
   (replacing an earlier item), else in the config-folder file. It prints
   where it stored it, never the key. `--file` stores it in the
   config-folder file on macOS too.
4. **`flankr key import gemini <file>`**: reads the key from that file
   and stores it as `set` does, then says the file can now be deleted; it
   does not delete it.
5. **`flankr key status`**: for each source in decision 1's order, whether
   a key is there (`found`, `none`, `locked`, `refused: <why>`), and which
   one the daemon would use; never any part of the key.
6. **The Keychain** is used through Apple's Security framework from Rust
   (the `security-framework` crate's generic-password functions), so the
   key never passes through a command line (`security
   add-generic-password -w <key>` would put it in the process list). On
   other systems the Keychain step is skipped.
7. **The start log** names the source used, once per backend:
   `[llm] gemini key: Keychain (dev.flankr/gemini)`, `… file
   ~/.config/flankr/secrets/gemini_api_key`, `… api_key_file <path>`, `…
   GEMINI_API_KEY`; or, with none, the sources looked at and
   `flankr key set gemini` as the fix. The key is never logged.
8. **Tests never touch the real Keychain or folders**: the store sits
   behind a small trait (`KeySource` with `get`/`set`), with a Keychain
   implementation and a file one; tests use the file one and a fake, under
   temporary `HOME`/`XDG_CONFIG_HOME`. The Keychain implementation is
   checked by hand on the Mac (step 3).
9. **Versions.** Where the daemon looks changes, which the person running
   it notices: the daemon takes the next minor with its `UPDATES.md`
   entry saying how to move an existing key (`flankr key import gemini
   .private/gemini_api_key`). The test configs that set `api_key_file`
   keep working (decision 1.1).

## 4. Steps

1. **The sources and the order**: decision 1, 2, 6, 7 and 8 in
   `src/llm/`, `.private`/`.env` reading removed, tests. **Done when:** unit
   tests cover each source in order, an `api_key_file` relative to its
   config file, a world-readable file refused, the start log's wording,
   and no key text in any log line.
2. **`flankr key`**: decisions 3, 4 and 5, usage text, the version and its
   `UPDATES.md` entry. **Done when:** integration tests (temporary HOME,
   the file store) cover `set` from stdin, `import`, `status`, an empty key
   refused, and that no output contains the key.
3. **On the Mac mini**: `flankr key import gemini .private/gemini_api_key`
   stores it in the Keychain; `flankr key status` says `Keychain: found`;
   the daemon started from another folder logs `gemini key: Keychain` and
   its summaries work. **Done when:** that is recorded in `CHANGES.md`.

## 5. Risks

- A daemon run over SSH with a locked login keychain finds no key there;
  the config-folder file (`flankr key set gemini --file`) covers it, and
  `status` says `locked`.
- macOS may ask once to allow flankr to read the item (a new binary after
  an update); the background daemon cannot answer that prompt. `key set`
  and `import`, run from a terminal, create the item with flankr allowed.

## Amendment — 2026-11-02 22:50 EST: approved

The owner approved the plan with the instruction that asked for it
(2026-11-02: "plan and implement"). It is built now; the first
implementing commit sets `In implementation`.
