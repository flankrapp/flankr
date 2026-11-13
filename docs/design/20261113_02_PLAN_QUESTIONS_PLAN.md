---
title: A plan's open questions, extracted by the planning plugin and answered inline, the answers sent to the plan's agent
date: 2026-11-13
status: Draft
covers:
  - "The planning plugin's reader finds a plan's `## Open questions` section
    and its numbered items: each item's number, bold lead, text, and the
    recommendation inside it; and which items a later amendment answered.
    `plans.show` carries them as `questions`, and `plans.list`'s rows an
    `open_questions` count"
  - "The app's plan page (the planning plugin's modal) gains an **Open
    questions** section on a `Draft` plan: each question's text drawn with
    flankr-markdown, and an answer row that opens a libcontrols `PopupMenu`
    of **Accept the recommendation**, **Write an answer…** (the message
    card's text area) and **Skip**; a **Send N answers** row at the end"
  - "The answers go as one owner message to the plan's agent through
    `agents.send`, `kind: message`, with its usual checks and confirmation,
    worded to pass the daemon's strict character filter and to name the
    plan by its date and title, as Build does; the app never writes to the
    plan file"
  - "An **N open questions** line on a Draft plan in Active tasks and in the
    plans table; answering a question never approves the plan"
  - "Not covered: parsing alternatives (\"A or B?\") into choice options
    (open question 3); answering by voice from the plan page (the message
    card's microphone works as it does today); questions in a plan past
    `Draft` (shown, not answerable); questions sourced from `tpt` (the
    external source gives no `questions`; the section then does not
    appear); changing libcontrols or flankr-markdown"
sources:
  - "Owner (2026-11-13, verbatim): \"plan to support extracting open
    questions from plans before approval and allow answering them inline.
    instead of weiting to the plan the answers will be submitted through
    the coding agent. this builds onthe existing markdiwn renderer and
    libcontrols\""
  - "flankr at `520974c` (branch `main-terminal`), read 2026-11-13:
    `src/plugins/planning/reader.rs` (`Plan`, `read_plan`: level-2
    headings and amendments only, fences skipped, cut at 64 KiB;
    `plan_json(plan, full)`), `src/plugins/planning/mod.rs` (`show`,
    `PluginVersion::Semver(\"0.1.2\")`, `via_tpt`), `src/plugins/policy/
    plans.rs` (`steps(text)`: a body parser of the same kind, for `## …
    Steps`), `src/commands.rs` (`validate_message`: control characters
    refused, whitespace folded to one space, a leading `/ ! #` refused,
    `MAX_MESSAGE_LEN` 4000; `validate_arbitrary_command`: 256,
    `SYMBOL_THRESHOLD` 0, `EXTRA_ALLOWED_CHARS`), `src/core/policy.rs`
    (`check_message`: `allow_messages`, `validate_message`, `check_paths`,
    confirmation always required), `crates/libflankr_proto/src/common.rs`
    (`AgentsSendParams`: `kind`, `confirmed`, `parts`), `crates/
    flankr-companion/src/policy.rs` (`build_request(date, title)`,
    `filter_clean`, `BUILD_MESSAGE_MAX`, `BUILD_STATUS_CLAUSE`),
    `crates/flankr-companion/src/client.rs` (`Input::SendMessage { pane_id,
    text, label }` sent as `SendKind::Message`), `apps/companion/src/
    plugins/planning.rs` (`PlanView`, `Page::Plan`, `lay_out_modal`,
    `Tap::Build`, a `PopupMenu` with `PopupMenuOptions`; \"nothing of a
    plan's body is shown\"), `apps/companion/src/plugins/mod.rs` (`Build`,
    `PluginOut::build`), `apps/companion/src/lib.rs` (`build_plan`:
    `agent_pane`, `TASKS_BUILD_NO_AGENT`, the policy plugin's `build` with
    `once`), `apps/companion/src/markdown_view.rs` (`MarkdownView` over
    `flankr_markdown::lay_out`), `apps/companion/copy/SETTINGS_COPY.md`"
  - "libcontrols 0.11.0 at the app's pinned revision, read 2026-11-13:
    `TextArea`, `Button` (`with_border`, `with_bg`), `SelectableLabel`,
    `PopupMenu` (`PopupItem::action`, `PopupRow`, `PopupMenuOptions`),
    `Options`, `Radio`; its own `markdown` module behind a feature the app
    does not use (the app uses flankr-markdown)"
  - "The open-questions sections of every plan in `docs/design/`, read
    2026-11-13: headings `## Open questions`, `## N. Open questions` and
    `## N. Open questions for the owner`; items `N. **Lead.** text`, often
    several lines, with `Recommend …`, `Recommended: …`, `Recommendation
    stays with …` and `(recommended)` inside; later amendments titled
    `… the open questions answered …`, `… approved with the open questions'
    recommendations` and `… open questions settled …`, and bodies naming
    `Open questions 1, 2 and 4 …`"
related:
  - "docs/design/20261027_04_GENERAL_AGENT_PLANNING_PLAN.md (the planning
    plugin and `plans.show`; the `plan_show` tool gains the questions too)"
  - "docs/design/20261028_04_PLUGINS_PLAN.md (decision 3.8: the plugin's
    device half, its modal and taps; the schema-1 shapes)"
  - "docs/design/20261024_04_MARKDOWN_RENDERER_PLAN.md (flankr-markdown and
    `MarkdownView`, which draw each question's text)"
  - "docs/design/20261023_02_BULK_INPUT_PLAN.md (the message class: the
    strict filter, 4000 characters, always confirmed, `allow_messages`;
    its 2026-10-27 13:25 EDT amendment, a tap as the approval)"
  - "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md (Build's message and
    the policy plugin's `build`; the plan's linked pane)"
  - "docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md (Active tasks,
    where a Draft plan's question count appears; its 23:23 EST amendment,
    Build from the plan page)"
  - "docs/design/20261029_01_CHOICE_QUESTIONS_PLAN.md (an agent's choice
    questions; the pattern for option rows if open question 3 is taken up)"
  - "docs/design/20261113_01_ADD_IMAGES_PLAN.md (the message card's input
    row, which **Write an answer…** reuses)"
---

# A plan's open questions, extracted and answered inline

## 1. Why

Every plan ends with a numbered list of open questions for the owner, and
most of them carry a recommendation. Today the owner reads them in the
preview or in the terminal, then tells the agent the answers in words, and
the agent writes them as an amendment under Rule 2. The app's plan page
shows a plan's title, status, covers and amendments, but nothing of its
body, so the questions are not reachable from the phone as questions.

This plan makes them reachable: the planning plugin extracts them, the plan
page lists them with an answer control each, and the answers go to the
plan's agent as one owner message, which the agent records as the Rule 2
amendment, quoting the owner. The app never writes to the plan: the agent
stays the only writer, the plan stays frozen or Draft as the rules say, and
approval stays a separate act (Build).

## 2. Findings (read 2026-11-13)

1. The planning reader keeps only level-2 headings and amendment headings
   (`read_plan`); the policy plugin's `plans.rs` has a body parser of the
   kind needed, `steps(text)`, for the Steps section. Neither reads the
   open questions.
2. `plans.show` is schema 1 and passes through unchanged when the source is
   `tpt` (`via_tpt`); a field added by the built-in reader is simply absent
   from a `tpt` answer.
3. The app's plan page (`Page::Plan`) draws plain wrapped lines into a
   `DrawList` with tap rects (`Tap::Build`, `Tap::Pin`), and can open a
   libcontrols `PopupMenu`. It does not host a `TextArea`; the message
   card (`apps/companion/src/prompt/`) does.
4. Build sends `Input::SendMessage { pane_id: agent_pane, text:
   build_request(date, title), label: "build" }`, which the client sends
   as `agents.send` with `kind: message`. The daemon's `check_message`
   requires `allow_messages`, runs `validate_message` (control characters
   refused, whitespace folded to one space, 4000 characters at most, no
   leading `/`, `!` or `#`) and `check_paths`, and always requires a
   confirmation; the companion's tap counts as the approval (the bulk
   input plan's 13:25 EDT amendment). `build_request` pre-cleans the title
   with `filter_clean` so the text passes the strict filter unchanged.
5. A message is one line when it reaches the pane: `validate_message`
   folds every run of whitespace, newlines included, to one space.
   `parts` exist for images, not for longer or multi-line text.
6. In the plans read, the section heading is `Open questions` with an
   optional `N. ` and an optional ` for the owner`; items are `N. ` at
   column 0 with indented continuation lines, usually opening with a
   `**bold lead.**`; recommendations are `Recommend …`, `Recommended: …`,
   `Recommendation …` or `(recommended)` after an alternative; the
   answering amendment's subject says `open questions` with `answered`,
   `settled` or `approved`, and some bodies name `Open questions 1, 2 and
   4`.

## 3. Decisions

1. **Extraction is the daemon's, in the planning plugin's reader**
   (`src/plugins/planning/reader.rs`), not the app's: one parser, tested
   against the real plans, and the general agent's `plan_show` tool gets
   the questions with it. The policy plugin does not change.

2. **Recognition.** In the body (after the front matter, before the first
   amendment, fences skipped as `fence()` skips them):
   - **The section** is the last level-2 heading whose text, after an
     optional `<digits>. ` prefix, starts with `open questions`
     (case-insensitive). Anything after it (` for the owner`) is kept as
     the heading's text.
   - **An item** starts at a line `^(\d+)\. ` at column 0 inside the
     section, and runs to the next such line, the next `## ` heading, or
     the end of the body. Indented lines and blank lines inside it are
     part of it, including `   - ` sub-bullets. The item's `text` is its
     lines with the leading indent of each continuation removed, joined
     with newlines, as Markdown.
   - **The lead** is a `**…**` at the very start of the item's text, as
     written, without the asterisks.
   - **The recommendation** is the first sentence of the item, after the
     lead, that starts with `recommend` (case-insensitive: `Recommend`,
     `Recommended:`, `Recommendation`), from its first letter to its
     ending `.`, `?` or the end of the item; `(recommended)` after an
     alternative gives the sentence that contains it. `null` when none.
   - **Answered.** An item is answered by the first amendment, in order,
     that answers it: every item, when the subject contains `open
     questions` with one of `answered`, `settled`, `approved`, or
     contains the word `answers` (`the owner's answers: …`), or the body
     has a sentence with `answered the open questions` and no numbers;
     the numbered items, when a body sentence has `open question` or
     `open questions` next to numbers (`Open questions 1, 2 and 4 …`,
     `answers open question 1`, `answers open questions 5 and 6`, `1
     (nested subagents) and 2`). A parenthesis between numbers is
     skipped. The amendment bodies are read for these sentence shapes
     only, outside fences.
   - A cut file (64 KiB) is read as far as it goes; `error` says `cut`,
     as today.

3. **The data shape.** `plans.show`'s plan gains, with `full`:

   ```json
   "questions": {
     "heading": "4. Open questions for the owner",
     "line": 212,
     "items": [
       { "n": 1, "line": 214,
         "lead": "Saving the choice.",
         "text": "**Saving the choice.** Save `home_view` in …",
         "recommendation": "Recommend no row: the label on the home screen is the control.",
         "answered": null },
       { "n": 2, "line": 218, "lead": "…", "text": "…", "recommendation": null,
         "answered": { "amendment": 3, "timestamp": "2026-11-12 21:58 EST" } }
     ]
   }
   ```

   `questions` is `null` when there is no section. `plans.list`'s rows
   gain `open_questions`, the count of items with `answered: null`, `0`
   with no section. Both are additive on schema 1 (a patch); a `tpt`
   answer has neither, and the app shows no section then.

4. **Where the answering happens: the plan page**, the planning plugin's
   modal, beside Build. The preview (Open in Active tasks) stays a
   read-only render. On a plan whose `status_base` is `Draft` the page
   gains, after Build and before `covers`, a section **Open questions**:
   - each unanswered item, drawn as `N.` then its `text` through the
     app's `MarkdownView` (`flankr_markdown::lay_out`, so the lead's bold
     and code spans render as in the preview); the plugin host's `Laid`
     gains a markdown block beside its plain `Line`s for this (step 3);
   - under it an **answer row**, a libcontrols `Button` drawn as an
     outline (`with_border`, transparent `with_bg`), reading
     **Answer ‹ not yet ›**, or the answer's summary once given
     (**Accepted the recommendation**, **Skipped**, or the first 40
     characters of the written text with …);
   - a tap on the row opens a `PopupMenu` anchored to it, of
     `PopupItem::action`s: **Accept the recommendation** (only when
     `recommendation` is not null), **Write an answer…**, **Skip**, and,
     once answered, **Clear**;
   - **Write an answer…** opens the message card's input row (its
     `TextArea`, keyboard and microphone as today) in an *answer* mode
     that shows a chip **Question N of <plan title>**; Submit stores the
     text as the question's answer and returns to the plan page, sending
     nothing; Cancel returns with nothing stored;
   - at the end of the section a row **Send N answers** (N the answered
     and skipped count, hidden at 0), with the dim line **Nothing is
     written to the plan; your agent records the answers** under it.
   On a plan past `Draft`, the section lists the items with their
   `answered` amendment's timestamp, and no rows. Answers in progress are
   kept in the plugin's memory for the app's run, keyed by the plan's
   base name; a `plans.show` re-read keeps them where the item numbers
   still match.

5. **The message.** **Send N answers** sends one owner message to the
   plan's agent, as Build does: `Input::SendMessage { pane_id, text,
   label: "answers" }`, so it goes as `agents.send` `kind: message` with
   the daemon's `allow_messages` check, `validate_message`, `check_paths`
   and confirmation, and the companion's tap counted as the approval. The
   text comes from a new `flankr_companion::policy::answers_request(date,
   title, answers) -> Option<String>`, built like `build_request`:

   ```text
   answers to the open questions of the 20261112 plan, Active plans a plan grouped home view: question 1: accept the recommendation. question 2: per agent, with the subagent under its parent. question 3: skipped. please record these answers as one amendment quoting them. this does not approve the plan
   ```

   - The plan is named by its front-matter date's eight digits and its
     title, never a path (no `/`).
   - Each answer is `question N: ` then `accept the recommendation`,
     `skipped`, or the written text passed through `filter_clean` (ASCII
     letters, digits and spaces; `'` between letters; `. , ? :` only
     after a letter or digit and before a space; anything else a space;
     runs of spaces folded), ending `. `.
   - The closing clause `ANSWERS_TAIL` is fixed text, so the agent knows
     to write the Rule 2 amendment and that nothing is approved.
   - The whole is one line (the daemon folds newlines anyway) of at most
     `ANSWERS_MAX = 4000` characters, the message class's cap. When the
     answers would exceed it, `answers_request` returns `None` and the
     app shows **Your answers are N characters too long** under the row,
     sending nothing; the owner shortens a written answer. There is no
     split into several messages: a second message would be confirmed
     apart from the first, and the agent could act on half the set.
   - Before sending, the page shows the exact text under **Send N
     answers**, as it will reach the pane, so what the owner confirms is
     what the agent reads.

6. **The agent.** The pane is the open agent view's (`agent_pane`), as
   Build's is; with no agent view open, the pane the policy plugin links
   to the plan (the plan policy agent plan's `linked_by`) when that
   plugin is active; else the note `TASKS_BUILD_NO_AGENT` reworded for
   answers. The agent writes the amendment under the project's Rule 2 in
   its own terms; the app does not check that it did. The plan page's
   next `plans.show` shows the items as answered once the amendment
   exists (decision 2's rule), and clears the kept answers.

7. **Approval stays separate.** Answering sends no `build`, arms nothing
   and changes no status; Build stays where it is on the plan page, above
   the questions. Where the owner wants both, they answer first, then
   Build; the agent sees two messages in order. The policy plugin and
   Active tasks do not learn of answers.

8. **Badges.** A Draft plan with `open_questions > 0` shows **N open
   questions** as a dim line: in Active tasks, under a plan's header
   (from `plans.list`, which the view already calls), a tap opening the
   plan page; and in the plans table, appended to the status cell as
   `Draft · 3` at the cell's small size. Plans past Draft show nothing.

9. **Copy.** New keys in `apps/companion/copy/SETTINGS_COPY.md`:
   `planning_questions` (**Open questions**), `planning_answer_not_yet`,
   `planning_answer_accepted`, `planning_answer_skipped`,
   `planning_menu_accept`, `planning_menu_write`, `planning_menu_skip`,
   `planning_menu_clear`, `planning_send_answers` (**Send {{ n }}
   answers**), `planning_answers_note`, `planning_answers_too_long`,
   `planning_answers_no_agent`, `planning_open_questions_badge`
   (**{{ n }} open questions**), `planning_answer_chip` (**Question {{ n
   }} of {{ title }}**).

10. **Versions**, all patches in the current series:
    - the planning plugin `0.1.2` → `0.1.3` (`questions` and
      `open_questions`, additive on schema 1);
    - the daemon `0.7.13` → `0.7.14`, with its `UPDATES.md` entry: the
      general agent's `plan_show` and the phone's plan page now carry a
      plan's open questions (FLANKR RULE 2);
    - flankr-companion `0.8.12` → `0.8.13` (`policy::answers_request`,
      `ANSWERS_MAX`, `ANSWERS_TAIL`; additive) with its `VERSIONS.md`
      entry;
    - the app, the next free patch after the one in implementation
      (`0.7.25` or later), with its `apps/UPDATES.md` entry (FLANKR RULE
      1; the owner has asked for patches only in this series);
    - libflankr_proto, libcontrols and flankr-markdown do not change.
    Nothing here is breaking.

## 4. Steps

1. **Daemon: the reader finds the questions.** In
   `src/plugins/planning/reader.rs`, `pub struct Questions { heading,
   line, items: Vec<Question> }` and `pub struct Question { n, line,
   lead, text, recommendation, answered: Option<Answered> }`, filled by
   `pub fn open_questions(lines: &[&str], body_start: usize, amendments:
   &[Amendment]) -> Option<Questions>` called from `read_plan`; `Plan`
   gains `questions`. `plan_json` writes `questions` with `full` and
   `open_questions` always. Tests in `reader.rs` on inline fixtures for:
   the three heading shapes; items of one and several lines with
   sub-bullets; a lead; the four recommendation shapes and none; a fence
   inside the section; the subject rule and the body-sentence rule for
   `answered`; no section. The planning fixture in `tests.rs` gains a
   plan with a section, and `plans_show_on_the_fixture` checks it.
   Plugin `0.1.3`; daemon `0.7.14`; `UPDATES.md`.
   **Done when:** `cargo test -p flankr` passes and `plans.show` on
   `20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN` gives nine items, seven with
   a recommendation (1, 2, 3, 4, 5, 7 and 8), items 1 to 6 `answered` by
   its 21:58 EST amendment (its body says `answers open question N` for
   1 to 4 and `answers open questions 5 and 6`), and the same call on
   `20261112_02_PLAN_POLICY_AGENT_PLAN` gives every item answered by its
   22:17 EST amendment (`approved by the owner, with the open questions
   settled`).

2. **flankr-companion: the message.** In `crates/flankr-companion/src/
   policy.rs`, `pub enum Answer { Accept, Skip, Text(String) }`, `pub fn
   answers_request(date: &str, title: &str, answers: &[(u32, Answer)]) ->
   Option<String>`, `ANSWERS_MAX`, `ANSWERS_TAIL`, sharing `filter_clean`
   with `build_request`. Tests: the example of decision 5 byte for byte;
   a written answer with `/`, `-`, quotes and a newline cleaned; `None`
   on a bad date, no answers, or over 4000; and the whole passing
   `validate_message`'s rules as the daemon states them (letters, digits,
   spaces, `' . , ? :`, no leading `/ ! #`). `0.8.13`; `VERSIONS.md`.
   **Done when:** `cargo test -p flankr-companion` passes with these.

3. **App: the plan page answers.** In `apps/companion/src/plugins/
   planning.rs`, `PlanView` gains `questions: Vec<QuestionView>` from
   `plans.show`; `Page::Plan` lays out decision 4's section; the plugin
   host's `Laid` gains a markdown block drawn with `MarkdownView`; new
   taps `Tap::Answer(n)`, `Tap::SendAnswers`; the `PopupMenu` of decision
   4 and its events; answers kept in `HashMap<String, Vec<(u32,
   Answer)>>`. `PluginOut` gains `answers(Answers { path, date, title,
   answers })` and `write_answer(n, title)`, and `lib.rs` an
   `answer_plan` beside `build_plan` that picks the pane (decision 6) and
   sends `Input::SendMessage { label: "answers" }`, and the message
   card's *answer* mode with its chip, whose Submit returns the text to
   the plugin. Copy keys of decision 9. App version and `apps/UPDATES.md`.
   **Done when:** on a Draft plan with questions, each item renders with
   its bold lead, the menu offers the three (or two) actions, a written
   answer returns to the page, **Send N answers** shows the exact text
   and sends one `agents.send` with `kind: message` whose text equals
   `answers_request`'s, and a plan past Draft shows the items with their
   amendment and no rows; `cargo test -p flankr-companion-app` (the app's
   crate) passes with layout tests for the section and the menu.

4. **App: the badges.** `PlanRow` gains `open_questions`; the plans
   table's status cell and Active tasks' plan header show decision 8's
   line, the header's tap opening the plan page as **Open** does. App
   version and `apps/UPDATES.md` (may share step 3's bump when landed
   together).
   **Done when:** a Draft plan with three unanswered questions shows
   `Draft · 3` in the table and **3 open questions** under its Active
   tasks header, and an Implemented plan shows neither.

## 5. Open questions for the owner

1. **Where the free-text answer is typed.** The plan page cannot host a
   `TextArea` today, so **Write an answer…** borrows the message card
   (decision 4). The alternative is a `TextArea` hosted in the plugin
   modal, which needs the plugin host to route keyboard events to a
   control. Recommend the message card: it already has the keyboard and
   the microphone, so answers can be dictated.
2. **One message or several.** Decision 5 refuses a set over 4000
   characters rather than splitting it. Accept, or allow a split into
   numbered messages (`answers 1 of 2 …`), each confirmed? Recommend
   refuse: 4000 characters is room for nine long answers.
3. **Alternatives as options.** Many questions read `A (recommended), or
   B?`. Parsing them into choice options (like the choice questions
   plan's option rows) is unreliable on the plans read; this plan offers
   only accept, write and skip. Take it up later, as a reader rule that
   splits one sentence ending `?` on ` or ` when both sides are under 80
   characters? Recommend later, once this page is in use.
4. **The agent pane with no agent view open.** Decision 6 falls back to
   the policy plugin's linked pane. Build does not (it needs the agent
   view); should both behave alike, and which way?
5. **The closing clause's wording** (`ANSWERS_TAIL`): "please record these
   answers as one amendment quoting them. this does not approve the
   plan". Is "quoting them" enough for the agent to quote the owner
   verbatim, as Rule 2 wants, or should the message say "under Rule 2"
   (the digit and word pass the filter)?
6. **The badge in the plans table.** `Draft · 3` in the status cell, a
   fifth column, or nothing in the table and only Active tasks? Recommend
   the status cell: no column widths change.
7. **Could not confirm in the time.** Whether the client's `send_to`
   strips newlines before `agents.send` or the daemon's `validate_message`
   refuses them as control characters (decision 5 sends one line either
   way, so only a written answer with a newline is affected: `filter_clean`
   makes it a space); whether the plugin host's `Laid` can carry a
   `MarkdownView` without changes to `apps/companion/src/plugins/mod.rs`
   beyond step 3, or the questions must fall back to plain wrapped lines
   with the lead in the text colour; whether the message card can be
   opened in a mode that returns its text to a plugin instead of sending
   (step 3 adds it if not); and the app crate's exact name for `cargo
   test -p` in step 3.

## Amendment — 2026-11-13 01:40 EST: approved by the owner, with the open questions settled

The owner approved this plan on 2026-11-13 ("yes", in answer to its
summary and open questions). Implementation has not started; the status
stays `Draft` until the first implementing commit. The open questions are
settled by the plan's own recommendations, and where it gave none, by
what its decisions already say:

1. A written answer is typed in the message card, in an answer mode that
   returns the text to the plan page and sends nothing.
2. A set of answers over 4000 characters is refused, not split.
3. Alternatives ("A or B") stay plain text; choice options are not built.
4. With no agent view open, answers fall back to the policy plugin's
   linked pane, as the decisions say; Build is unchanged by this plan.
5. The closing clause stays as written ("please record these answers as
   one amendment quoting them. this does not approve the plan").
6. The count shows in the plans table's status cell, `Draft · N`, and
   under the plan's header in Active tasks.
7. The details not confirmed while writing are settled while building,
   and any departure is recorded in `CHANGES.md`.
