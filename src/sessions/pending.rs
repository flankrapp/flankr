//! An agent's pending question (plan 20261022_03, decision 8).
//!
//! The asking paragraph of the agent's last assistant `Text` entry is its
//! pending question when:
//! - the final paragraph asks: it ends with `?`, has a line that does, or
//!   has a sentence opening with a go-ahead invitation ("Say go and
//!   I'll …"); or the final paragraph is a one-line closing note and the
//!   one before it opens a sentence with such an invitation (the
//!   2026-11-13 09:50 EST amendment);
//! - no user entry follows that entry (an entry with an `origin`, such as
//!   a task notification or a recap, is context, not the user answering);
//!   and
//! - the pane's `agent_status` is `idle`, `blocked` or `done` (herdr marks a
//!   pane `done` when its agent has finished a turn the user has not yet
//!   looked at, which is when a question asked at the end of a task waits);
//!   or, whatever herdr says, the turn has visibly ended: that reply is the
//!   session's last entry but for context and the file has been quiet for
//!   [`TURN_QUIET`] (herdr reports a Claude Code pane `working` while a background agent
//!   of its runs, after its turn has ended).
//!
//! The question is redacted, then cut to `PENDING_QUESTION_MAX`
//! characters. Asked for its answers (an [`Asker`]), it gets them from
//! `SUGGESTIONS.md`'s rules (plan 20261027_06, step 3), and is marked
//! `about_plan`, with `plan` its `detected_plan_id`, when the `plan` rule
//! took it. Otherwise it has no answers and is marked by
//! [`super::links::plan_question`] (the 21:57 EDT amendment).

use libherdr::protocol::AgentStatus;

use super::redact::redact;
use libflankr_proto::common::{
    CHOICE_DETAIL_MAX, CHOICE_HEADER_MAX, CHOICE_LABEL_MAX, CHOICE_PREVIEW_MAX, ChoiceOption,
    ChoiceQuestion, ChoiceSet,
};

use super::{EntryKind, Item, PENDING_QUESTION_MAX, PendingQuestion, Role, cut};
use crate::suggestions::{Suggestions, Who};

/// Who a question's answers are for, and the rules that pick them.
#[derive(Debug, Clone, Copy)]
pub struct Asker<'a> {
    pub suggestions: &'a Suggestions,
    pub who: Who<'a>,
}

/// How long the session file must be unwritten before a final reply counts
/// as the end of the turn whatever herdr's status.
pub const TURN_QUIET: std::time::Duration = std::time::Duration::from_secs(5);

/// The pending question in `items` (oldest first) for an agent whose
/// status is `status`, if it has one. `quiet` is whether the session file
/// has been unwritten for [`TURN_QUIET`]. With an `asker`, the question
/// carries its answers.
pub fn pending_question(
    items: &[Item],
    status: &AgentStatus,
    quiet: bool,
    asker: Option<&Asker>,
) -> Option<PendingQuestion> {
    if let Some(at) = open_choice(items, status, quiet) {
        return choice_question(&items[at]);
    }
    let last = items
        .iter()
        .rposition(|i| i.role == Role::Assistant && i.kind == EntryKind::Text)?;
    let waiting = matches!(
        status,
        AgentStatus::Idle | AgentStatus::Blocked | AgentStatus::Done
    );
    // Context (a notification, a recap) after the reply neither answers
    // it nor means the agent went on.
    let after = &items[last + 1..];
    let ended = quiet && after.iter().all(|i| i.origin.is_some());
    if !waiting && !ended {
        return None;
    }
    if after.iter().any(Item::is_user_said) {
        return None;
    }
    question_for(&items[last], asker)
}

/// The index in `items` of the open choice question (plan 20261029_01,
/// decision 4): the newest tool call with `choices` whose `call_id` has no
/// result after it, when only context follows it and the pane waits or
/// the file is quiet.
pub(crate) fn open_choice(items: &[Item], status: &AgentStatus, quiet: bool) -> Option<usize> {
    let at = items
        .iter()
        .rposition(|i| i.kind == EntryKind::ToolUse && i.choices.is_some())?;
    let id = items[at].call_id.as_deref()?;
    let after = &items[at + 1..];
    if after
        .iter()
        .any(|i| i.kind == EntryKind::ToolResult && i.call_id.as_deref() == Some(id))
    {
        return None;
    }
    if !after.iter().all(|i| i.origin.is_some()) {
        return None;
    }
    let waiting = matches!(
        status,
        AgentStatus::Idle | AgentStatus::Blocked | AgentStatus::Done
    );
    (waiting || quiet).then_some(at)
}

/// The pending question of a call with `choices`: every text redacted and
/// cut to its bound, `text` the first question's. `SUGGESTIONS.md`'s rules
/// are not applied, and it has no `answers`.
pub(crate) fn choice_question(call: &Item) -> Option<PendingQuestion> {
    let set = call.choices.as_ref()?;
    let shown = |s: &str, max: usize| cut(&redact(s), max);
    let questions = set
        .questions
        .iter()
        .map(|q| {
            let options = q
                .options
                .iter()
                .map(|o| {
                    ChoiceOption::new(
                        shown(&o.label, CHOICE_LABEL_MAX),
                        shown(&o.detail, CHOICE_DETAIL_MAX),
                        o.preview.as_deref().map(|p| shown(p, CHOICE_PREVIEW_MAX)),
                    )
                })
                .collect();
            ChoiceQuestion::new(
                shown(&q.header, CHOICE_HEADER_MAX),
                shown(&q.text, PENDING_QUESTION_MAX),
                q.multi,
                q.other,
                options,
            )
        })
        .collect::<Vec<_>>();
    let text = questions.first()?.text.clone();
    Some(PendingQuestion {
        text,
        ts: call.ts.clone(),
        about_plan: false,
        plan: None,
        links: Vec::new(),
        answers: Vec::new(),
        choices: Some(ChoiceSet::new(
            set.id.clone(),
            questions,
            set.source.clone(),
        )),
    })
}

/// The question a reply asks, whether or not it waits: its final
/// paragraph when that asks, redacted, cut and marked about a plan. The
/// sections' resolved Question cards come from it (plan 20261023_03, A3).
pub fn question_in(reply: &Item) -> Option<PendingQuestion> {
    question_for(reply, None)
}

/// [`question_in`], with its answers when there is an `asker`.
pub fn question_for(reply: &Item, asker: Option<&Asker>) -> Option<PendingQuestion> {
    let paragraph = asking_paragraph(&reply.text)?;
    let redacted = redact(paragraph);
    // An id the question lacks may be earlier in the reply.
    let context = redact(&reply.text);
    let (about_plan, plan, answers) = match asker {
        Some(a) => match a.suggestions.offer(&redacted, &context, a.who) {
            Some(offered) => {
                let about_plan = offered.about_plan();
                let answers = offered.answers.into_iter().map(Into::into).collect();
                (about_plan, about_plan.then_some(offered.plan), answers)
            }
            None => (false, None, Vec::new()),
        },
        None => {
            let (about_plan, plan) = super::links::plan_question(&redacted, &context);
            (about_plan, plan, Vec::new())
        }
    };
    Some(PendingQuestion {
        text: cut(&redacted, PENDING_QUESTION_MAX),
        ts: reply.ts.clone(),
        about_plan,
        plan,
        links: Vec::new(),
        answers,
        choices: None,
    })
}

/// The longest closing note passed over to the paragraph before it, in
/// characters (the 2026-11-13 09:50 EST amendment, item 1).
const CLOSING_NOTE_MAX: usize = 160;

/// Sentence openings that ask the owner for a go-ahead without a `?`
/// (the 09:50 EST amendment, item 2), lower case.
const INVITATIONS: [&str; 12] = [
    "say go",
    "say yes",
    "say the word",
    "say so",
    "shall i ",
    "should i ",
    "want me to ",
    "do you want me to ",
    "would you like me to ",
    "let me know if you want",
    "let me know whether",
    "tell me whether",
];

/// The paragraph of `text` that asks: its final paragraph when that
/// asks, else the one before it when the final one is a closing note and
/// that one asks for a go-ahead (the 09:50 EST amendment, item 1).
fn asking_paragraph(text: &str) -> Option<&str> {
    let paragraphs = paragraphs(text);
    let (&last, before) = paragraphs.split_last()?;
    if asks(last) {
        return Some(last);
    }
    let &previous = before.last()?;
    (closing_note(last) && sentences(previous).any(invites)).then_some(previous)
}

/// The paragraphs of `text`, trimmed, split at blank lines.
fn paragraphs(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            if let Some(s) = start.take() {
                out.push(text[s..offset].trim());
            }
        } else if start.is_none() {
            start = Some(offset);
        }
        offset += line.len();
    }
    if let Some(s) = start {
        out.push(text[s..].trim());
    }
    out
}

/// Whether `paragraph` is a closing note: one short line of prose.
fn closing_note(paragraph: &str) -> bool {
    let numbered = paragraph
        .split_once(". ")
        .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    !paragraph.contains('\n')
        && paragraph.chars().count() <= CLOSING_NOTE_MAX
        && !paragraph.starts_with(['#', '|'])
        && !paragraph.starts_with("- ")
        && !paragraph.starts_with("* ")
        && !paragraph.starts_with("```")
        && !numbered
}

/// Whether the paragraph asks: it ends with `?` or has a line that does
/// (markdown emphasis and closing quotes or brackets after the `?` do not
/// count), or a sentence of it opens with a go-ahead invitation.
fn asks(paragraph: &str) -> bool {
    paragraph.lines().any(|line| {
        line.trim_end()
            .trim_end_matches(['*', '_', '`', '"', '\'', ')', ']', '”', '’'])
            .ends_with('?')
    }) || sentences(paragraph).any(invites)
}

/// The sentences of `paragraph`: split at line ends and after `. `, `! `,
/// `? `, `: ` and ` — `.
fn sentences(paragraph: &str) -> impl Iterator<Item = &str> {
    paragraph.lines().flat_map(|line| {
        let mut out = Vec::new();
        let mut start = 0;
        for (i, _) in line.char_indices() {
            let rest = &line[i..];
            let cut = if [". ", "! ", "? ", ": "].iter().any(|p| rest.starts_with(p)) {
                Some(2)
            } else if rest.starts_with(" — ") {
                Some(" — ".len())
            } else {
                None
            };
            if let Some(n) = cut
                && i >= start
            {
                out.push(&line[start..i]);
                start = i + n;
            }
        }
        out.push(&line[start..]);
        out
    })
}

/// Whether `sentence` opens with one of [`INVITATIONS`].
fn invites(sentence: &str) -> bool {
    let lower = sentence
        .trim_start()
        .trim_start_matches(['*', '_', '>', ' '])
        .to_lowercase();
    INVITATIONS.iter().any(|w| {
        lower.starts_with(w)
            && (w.ends_with(' ')
                || lower[w.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionReader;
    use crate::sessions::claude::ClaudeCode;
    use crate::sessions::test_support::fixture;

    fn text(role: Role, ts: &str, text: &str) -> Item {
        Item {
            pos: 0,
            ts: ts.to_string(),
            role,
            kind: EntryKind::Text,
            tool: None,
            text: text.to_string(),
            call_id: None,
            origin: None,
            choices: None,
            choice_result: None,
        }
    }

    fn tool(role: Role, kind: EntryKind) -> Item {
        Item {
            pos: 0,
            ts: "t".to_string(),
            role,
            kind,
            tool: Some("Bash".to_string()),
            text: "Bash {}".to_string(),
            call_id: None,
            origin: None,
            choices: None,
            choice_result: None,
        }
    }

    #[test]
    fn the_last_reply_ending_in_a_question_is_pending() {
        // The shape of the 2026-10-23 transcript's end: a long reply whose
        // last paragraph asks.
        let path = fixture("claude_question.jsonl");
        let items = ClaudeCode::new(fixture("x")).tail(&path, 50).unwrap();
        let q = pending_question(&items, &AgentStatus::Idle, false, None).unwrap();
        assert_eq!(
            q.text,
            "Should the notices wait for the phone to finish speaking, or interrupt it?"
        );
        assert_eq!(q.ts, "2026-01-04T07:15:00.000Z");
        // Blocked counts too; working does not.
        assert!(pending_question(&items, &AgentStatus::Blocked, false, None).is_some());
        assert!(pending_question(&items, &AgentStatus::Working, false, None).is_none());
        // A task that ends with a question: herdr says done until it is seen.
        assert!(pending_question(&items, &AgentStatus::Done, false, None).is_some());
    }

    #[test]
    fn a_following_user_entry_clears_it() {
        let mut items = vec![
            text(Role::User, "t1", "do it"),
            text(Role::Assistant, "t2", "Done.\n\nShall I push?"),
        ];
        assert!(pending_question(&items, &AgentStatus::Idle, false, None).is_some());
        // Tool entries after it do not clear it.
        items.push(tool(Role::Assistant, EntryKind::ToolUse));
        items.push(tool(Role::Tool, EntryKind::ToolResult));
        assert!(pending_question(&items, &AgentStatus::Idle, false, None).is_some());
        items.push(text(Role::User, "t3", "yes"));
        assert_eq!(
            pending_question(&items, &AgentStatus::Idle, false, None),
            None
        );
    }

    #[test]
    fn context_after_the_question_does_not_answer_it() {
        let mut items = vec![
            text(Role::User, "t1", "do it"),
            text(Role::Assistant, "t2", "Done.\n\nShall I push?"),
        ];
        for origin in ["task", "peer:docs-writer", "agent:a1", "recap", "compact"] {
            let mut notice = text(Role::User, "t3", "Build finished?");
            notice.origin = Some(origin.to_string());
            items.push(notice);
        }
        assert_eq!(
            pending_question(&items, &AgentStatus::Idle, false, None)
                .unwrap()
                .text,
            "Shall I push?"
        );
        // Quiet with only context after the reply: the turn has ended.
        assert!(pending_question(&items, &AgentStatus::Working, true, None).is_some());
        // The user's own words answer it.
        items.push(text(Role::User, "t4", "yes"));
        assert_eq!(
            pending_question(&items, &AgentStatus::Idle, false, None),
            None
        );
    }

    #[test]
    fn a_task_notification_read_from_the_file_leaves_the_question_pending() {
        // The context fixture ends on a question; put a queued notification
        // after it.
        let mut body = std::fs::read_to_string(fixture("claude_context.jsonl")).unwrap();
        body.push_str(concat!(
            r#"{"parentUuid":"u","isSidechain":false,"attachment":{"type":"queued_command","#,
            r#""prompt":"<task-notification>\n<summary>Tests passed</summary>\n</task-notification>","#,
            r#""commandMode":"task-notification","origin":{"kind":"task-notification"},"#,
            r#""timestamp":"2026-01-05T09:00:20.000Z"},"type":"attachment"}"#,
            "\n"
        ));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(&path, body).unwrap();
        let items = ClaudeCode::new(fixture("x")).tail(&path, 50).unwrap();
        assert_eq!(items.last().unwrap().origin.as_deref(), Some("task"));
        let q = pending_question(&items, &AgentStatus::Idle, false, None).unwrap();
        assert_eq!(q.text, "Done. Shall I push?");
    }

    #[test]
    fn only_the_final_paragraph_counts() {
        let asked_earlier = vec![text(
            Role::Assistant,
            "t",
            "Did that work?\n\nIt did. Everything is committed.",
        )];
        assert_eq!(
            pending_question(&asked_earlier, &AgentStatus::Idle, false, None),
            None
        );
        // A line of the final paragraph that asks, with markdown after it.
        let list = vec![text(
            Role::Assistant,
            "t",
            "Summary.\n\n1. **Keep the old name?**\n2. Rename it everywhere.",
        )];
        assert_eq!(
            pending_question(&list, &AgentStatus::Blocked, false, None)
                .unwrap()
                .text,
            "1. **Keep the old name?**\n2. Rename it everywhere."
        );
        assert_eq!(
            pending_question(
                &[text(Role::Assistant, "t", "\n\n  \n")],
                &AgentStatus::Idle,
                false,
                None
            ),
            None
        );
        assert_eq!(pending_question(&[], &AgentStatus::Idle, false, None), None);
    }

    #[test]
    fn the_question_is_redacted_and_cut() {
        let key = "AIzaSyD0nTuSeTh1sFakeKeyAbCdEfGhIjKlMn";
        let items = vec![text(
            Role::Assistant,
            "t",
            &format!("Is {key} the key you meant? {}", "x".repeat(600)),
        )];
        // The `?` is mid-line: this line does not end with one.
        assert_eq!(
            pending_question(&items, &AgentStatus::Idle, false, None),
            None
        );
        let items = vec![text(
            Role::Assistant,
            "t",
            &format!("Is {key} the key? {} Or another?", "x".repeat(600)),
        )];
        let q = pending_question(&items, &AgentStatus::Idle, false, None).unwrap();
        assert!(!q.text.contains(key));
        assert_eq!(q.text.chars().count(), PENDING_QUESTION_MAX);
        assert!(q.text.ends_with('…'));
    }

    #[test]
    fn a_question_ending_a_quiet_turn_is_pending_while_herdr_says_working() {
        // herdr says working while a background agent runs; the session
        // shows the turn ended on a question.
        let asked = [
            text(Role::User, "1", "go"),
            text(Role::Assistant, "2", "Built it.\n\nInstall it on the S23?"),
        ];
        assert_eq!(
            pending_question(&asked, &AgentStatus::Working, true, None)
                .unwrap()
                .text,
            "Install it on the S23?"
        );
        // Not quiet yet: still working, maybe.
        assert!(pending_question(&asked, &AgentStatus::Working, false, None).is_none());
        // Quiet, but the reply is not the last entry: the agent went on.
        let went_on = [
            asked[0].clone(),
            asked[1].clone(),
            tool(Role::Assistant, EntryKind::ToolUse),
        ];
        assert!(pending_question(&went_on, &AgentStatus::Working, true, None).is_none());
    }

    #[test]
    fn a_question_about_a_plan_says_so_and_names_it() {
        let asked = [text(
            Role::Assistant,
            "2",
            "Wrote the amendment.\n\nApprove plan 04 and build it now?",
        )];
        let q = pending_question(&asked, &AgentStatus::Idle, false, None).unwrap();
        assert!(q.about_plan);
        assert_eq!(q.plan.as_deref(), Some("plan 04"));
        assert!(q.links.is_empty(), "links are the server's to add");
        let plain = [text(Role::Assistant, "2", "Install it on the S23?")];
        let q = pending_question(&plain, &AgentStatus::Idle, false, None).unwrap();
        assert!(!q.about_plan);
        assert_eq!(q.plan, None);
    }

    /// The replies of the owner's report of 2026-11-13 09:47 EST, cut
    /// down: go-ahead questions with no `?`, some before a closing note.
    const SAY_THE_WORD: &str = "**My recommendation, in order:** the catch-up amendments, then the reader plan.\n\nWhere I left off: one plan is closed, and I was partway into writing a round-trip test \u{2014} worth finishing either way, since it's one test on a plan that's otherwise done. Say the word and I'll carry on with that, or switch to whichever of the four above you want first.";
    const SAY_GO_THEN_NOTE: &str = "## If you want it\n\nIt's a rules change, so it goes in the rules repository first.\n\nSay go and I'll draft it there: the Rule 1.3 entry, the Rule 1.2 line about plan length, and the `CHANGES.md` addendum.\n\nStill holding the layer round-trip test, unstarted on disk.";
    const NO_ASK: &str = "## Fixed on the way through\n\nOne README row is stale but I left it. That cell is editorial and yours to write.\n\nI read code and documents for this; I ran no tests and rendered nothing.";

    #[test]
    fn a_go_ahead_without_a_question_mark_is_the_pending_question() {
        let items = [
            text(Role::User, "1", "go"),
            text(Role::Assistant, "2", SAY_THE_WORD),
        ];
        let q = pending_question(&items, &AgentStatus::Done, false, None).unwrap();
        assert!(q.text.starts_with("Where I left off:"), "{}", q.text);
        assert!(q.text.ends_with("you want first."));
        assert_eq!(q.ts, "2");
        for reply in [
            "Done.\n\nSay go and I'll draft it.",
            "Done.\n\nShall I push it to the S23.",
            "Done.\n\n**Want me to build it now.**",
            "Done.\n\nThe tests pass \u{2014} say yes and I'll push.",
            "Done.\n\nLet me know whether to push.",
        ] {
            let items = [text(Role::Assistant, "2", reply)];
            assert!(
                pending_question(&items, &AgentStatus::Idle, false, None).is_some(),
                "{reply}"
            );
        }
    }

    #[test]
    fn a_go_ahead_before_a_closing_note_is_the_pending_question() {
        let items = [text(Role::Assistant, "2", SAY_GO_THEN_NOTE)];
        let q = pending_question(&items, &AgentStatus::Idle, false, None).unwrap();
        assert!(
            q.text.starts_with("Say go and I'll draft it there:"),
            "{}",
            q.text
        );
        assert!(!q.text.contains("Still holding"));
        let items = [text(
            Role::Assistant,
            "2",
            "Built it.\n\nShall I install it on the S23?\n\nThe tests pass.",
        )];
        assert_eq!(
            pending_question(&items, &AgentStatus::Idle, false, None)
                .unwrap()
                .text,
            "Shall I install it on the S23?"
        );
    }

    #[test]
    fn a_go_ahead_gets_the_yes_and_no_answers() {
        let (suggestions, _) = Suggestions::shipped();
        let asker = Asker {
            suggestions: &suggestions,
            who: Who {
                agent: "Claude",
                project: "elsewhere",
            },
        };
        let items = [text(Role::Assistant, "2", SAY_GO_THEN_NOTE)];
        let q = pending_question(&items, &AgentStatus::Idle, false, Some(&asker)).unwrap();
        let sends: Vec<_> = q.answers.iter().map(|a| a.sends.as_deref()).collect();
        assert_eq!(sends, [Some("yes"), Some("no")]);
        assert!(!q.about_plan);
    }

    #[test]
    fn what_does_not_ask_for_a_go_ahead_is_not_a_question() {
        for reply in [
            NO_ASK,
            "Done.\n\nLet me know if anything breaks.",
            "Done.\n\nSay something about the logs: they are long.",
            "Done.\n\nThe shall-I list is empty.",
            // A question the next paragraph answers is not pending.
            "Did that work?\n\nIt did. Everything is committed.",
            // A closing paragraph of two lines, or a long one, is not a note.
            "Say go and I'll push.\n\nStill holding the test.\nAnd the render.",
            &format!(
                "Say go and I'll push.\n\nStill holding {}.",
                "x".repeat(160)
            ),
            // A list or a heading after the go-ahead is not a note either.
            "Say go and I'll push.\n\n- the test",
            "Say go and I'll push.\n\n2. the test",
            "Say go and I'll push.\n\n## Next",
            // Only one note is passed over.
            "Say go and I'll push.\n\nOne note.\n\nTwo notes.",
        ] {
            let items = [text(Role::Assistant, "2", reply)];
            assert_eq!(
                pending_question(&items, &AgentStatus::Idle, false, None),
                None,
                "{reply}"
            );
        }
    }

    #[test]
    fn a_go_ahead_is_a_live_question_card_not_a_finished_one() {
        use crate::sessions::links::Linker;
        use crate::sessions::sections::{self, Live, Placed};
        use libflankr_proto::common::CardKind;
        let linker = Linker::new(None, None);
        for reply in [SAY_THE_WORD, SAY_GO_THEN_NOTE, NO_ASK] {
            let items = [
                text(Role::User, "1", "go"),
                text(Role::Assistant, "2", reply),
            ];
            let placed: Vec<Placed> = items
                .iter()
                .enumerate()
                .map(|(n, item)| Placed {
                    line: n as u32 + 1,
                    slot: 0,
                    item: item.clone(),
                })
                .collect();
            let status = AgentStatus::Done;
            let q = pending_question(&items, &status, false, None);
            let live = Live {
                name: "Claude",
                status: &status,
                quiet: false,
                pending_question: q.as_ref(),
                blocked_at: None,
            };
            let kinds: Vec<_> = sections::cards(7, &placed, &[], Some(&live), &linker)
                .into_iter()
                .filter_map(|s| s.card.map(|c| (c.card, c.live)))
                .collect();
            let want = if reply == NO_ASK {
                CardKind::Finished
            } else {
                CardKind::Question
            };
            assert_eq!(kinds, [(want, true)], "{reply}");
        }
    }
}
