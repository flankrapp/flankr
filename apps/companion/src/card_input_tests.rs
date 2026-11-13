//! Typing into the prompt cards while cards come and go, in scripted runs
//! of the whole app (the owner's reports of 2026-11-03 13:05 and 13:06 EST
//! in `docs/OWNER_REPORTS.md`), as `pinned_cards_tests` drives them: the
//! view of p1 opened from the drawer with one card pinned, the client's
//! events given by the test, and what the app did read back from
//! [`probe`]. Each run needs a GPU adapter; without one it says so and
//! passes.

use flankr_companion::{AgentCard, CardKind, Event, PromptMode, Shows, ViewSection};
use libflankr_proto::common::Serial;

use super::pinned_cards_tests::{SETTLE, inputs_with, live, open_view, run_with, tap, view};
use super::probe::Probe;
use crate::agent::AgentHit;

/// Frames past the 500 ms hold, at 15 fps.
const HOLD: u32 = 12;

/// `view(card)` with the card at a serial of its own, after the first
/// card's: a new card, pinned.
fn new_card_view(card: AgentCard) -> Event {
    let mut event = view(card);
    if let Event::AgentView {
        sections,
        pinned,
        changed,
        ..
    } = &mut event
    {
        let serial = Serial::new(7, 11, 0);
        if let Some(ViewSection {
            serial: s,
            shows: Shows::Card(_),
            ..
        }) = sections.last_mut()
        {
            *s = serial;
        }
        *pinned = Some(serial);
        *changed = sections.iter().map(|s| s.serial).collect();
    }
    event
}

fn bar_record(p: &Probe) -> libcontrols::Rect {
    p.drawer
        .bar
        .iter()
        .find(|(h, _)| *h == AgentHit::Message)
        .map(|&(_, r)| r)
        .expect("the bar's record")
}

fn hold(r: libcontrols::Rect) -> String {
    let [x, y] = r.center();
    format!("touch 5 start {x} {y}\nframes {HOLD}\ntouch 5 end {x} {y}\nframes 4\n")
}

#[test]
fn a_new_pinned_card_while_typing_keeps_the_message_card_its_text_and_focus() {
    let first = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, p, _)) = open_view(&first) else {
        return;
    };
    let script = format!(
        "{open}{}key h\nframes 2\nkey i\nframes {SETTLE}\nframes {SETTLE}\n",
        hold(bar_record(&p))
    );
    let approval = live(CardKind::Approval, "Claude needs approval", "cargo test");
    let p = run_with(
        &script,
        &first,
        vec![("SetText(\"hi\")", new_card_view(approval))],
    )
    .unwrap();
    assert_eq!(inputs_with(&p, "StartTextPrompt"), 1, "{:?}", p.inputs);
    assert_eq!(
        p.drawer.prompt,
        Some((false, PromptMode::Keyboard, "hi".to_string())),
        "{:?}",
        p.inputs
    );
    assert!(p.drawer.focused.is_some());
}

#[test]
fn a_confirmation_or_another_sessions_card_while_typing_keeps_the_message_card() {
    let first = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, p, _)) = open_view(&first) else {
        return;
    };
    let script = format!(
        "{open}{}key h\nframes 2\nkey i\nframes {SETTLE}\nframes {SETTLE}\n",
        hold(bar_record(&p))
    );
    let cross = flankr_companion::SessionCard {
        pane_id: "p2".into(),
        session: "s2".into(),
        kind: flankr_companion::SessionCardKind::Question,
        agent: "Codex".into(),
        colour: 3,
        text: "Merge it?".into(),
        at: std::time::Duration::from_secs(1),
    };
    let p = run_with(
        &script,
        &first,
        vec![
            (
                "SetText(\"hi\")",
                Event::Confirm {
                    at: None,
                    pane_id: String::new(),
                    requested_by: String::new(),
                    confirm_id: 9,
                    text: "Approve?".into(),
                    label: Some("x".into()),
                    phrase: String::new(),
                    command: "cargo test".into(),
                    agent: "Claude".into(),
                    project: None,
                },
            ),
            ("SetText(\"hi\")", Event::SessionCards(vec![cross])),
        ],
    )
    .unwrap();
    assert_eq!(
        p.drawer.prompt,
        Some((false, PromptMode::Keyboard, "hi".to_string())),
        "{:?}",
        p.inputs
    );
    assert!(p.drawer.focused.is_some());
}

#[test]
fn the_voice_message_cards_keyboard_goes_on_typed_and_focused() {
    let first = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, p, _)) = open_view(&first) else {
        return;
    };
    // A tap on record: the message card by voice, with its keyboard glyph.
    let script = format!("{open}{}frames {SETTLE}\n", tap(bar_record(&p)));
    let p = run_with(&script, &first, Vec::new()).unwrap();
    assert_eq!(inputs_with(&p, "StartDictation"), 1, "{:?}", p.inputs);
    assert_eq!(
        p.drawer.prompt.as_ref().map(|d| (d.0, d.1)),
        Some((false, PromptMode::Voice))
    );
    let keyboard = p
        .drawer
        .voice_keyboards
        .iter()
        .find(|(k, _)| *k == crate::prompt::PromptKind::Message)
        .map(|&(_, r)| r)
        .expect("the voice card's keyboard");
    // Its tap: the same message, typed and focused, with no new prompt.
    let script = format!("{script}{}frames {SETTLE}\n", tap(keyboard));
    let p = run_with(&script, &first, Vec::new()).unwrap();
    assert_eq!(inputs_with(&p, "ContinueByKeyboard"), 1, "{:?}", p.inputs);
    assert_eq!(inputs_with(&p, "StartTextPrompt"), 0, "{:?}", p.inputs);
    assert_eq!(
        p.drawer.prompt.as_ref().map(|d| (d.0, d.1)),
        Some((false, PromptMode::Keyboard))
    );
    assert_eq!(
        p.drawer.focused,
        Some(crate::prompt::keyboard::FocusedPrompt::Card {
            screen: crate::prompt::keyboard::PromptScreen::Agent,
            kind: crate::prompt::PromptKind::Message,
        })
    );
    assert!(p.drawer.voice_keyboards.is_empty(), "typed: no glyph");
}

/// The pinned question's record, held `frames` before the release.
fn press(r: libcontrols::Rect, frames: u32) -> String {
    let [x, y] = r.center();
    format!("touch 6 start {x} {y}\nframes {frames}\ntouch 6 end {x} {y}\nframes {SETTLE}\n")
}

#[test]
fn a_500_ms_hold_on_a_question_cards_record_opens_its_reply_typed() {
    let card = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, p, _)) = open_view(&card) else {
        return;
    };
    let record = p.drawer.reply.expect("the card's record");
    // Held past 500 ms: the reply, typed and focused, nothing listening.
    let p = run_with(&format!("{open}{}", press(record, HOLD)), &card, Vec::new()).unwrap();
    assert_eq!(inputs_with(&p, "StartTextPrompt"), 1, "{:?}", p.inputs);
    assert_eq!(inputs_with(&p, "StartDictation"), 0, "a hold is no tap");
    assert_eq!(
        p.drawer.prompt.as_ref().map(|d| (d.0, d.1)),
        Some((false, PromptMode::Keyboard))
    );
    assert_eq!(p.drawer.replying_to.as_deref(), Some("Claude is asking"));
    assert_eq!(
        p.drawer.focused,
        Some(crate::prompt::keyboard::FocusedPrompt::Card {
            screen: crate::prompt::keyboard::PromptScreen::Agent,
            kind: crate::prompt::PromptKind::Message,
        })
    );
    assert!(!p.drawer.holding);
    // Released before: the voice reply, as before.
    let p = run_with(&format!("{open}{}", press(record, 3)), &card, Vec::new()).unwrap();
    assert_eq!(inputs_with(&p, "StartDictation"), 1, "{:?}", p.inputs);
    assert_eq!(inputs_with(&p, "StartTextPrompt"), 0);
    assert_eq!(
        p.drawer.prompt.as_ref().map(|d| (d.0, d.1)),
        Some((false, PromptMode::Voice))
    );
    assert_eq!(p.drawer.replying_to.as_deref(), Some("Claude is asking"));
}

#[test]
fn nothing_is_drawn_along_the_record_while_it_is_held() {
    let card = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, p, _)) = open_view(&card) else {
        return;
    };
    let record = p.drawer.reply.expect("the card's record");
    let [x, y] = record.center();
    // Half-way through the hold: timed, and the screen is as before it.
    let script = format!("{open}touch 6 start {x} {y}\nframes 3\n");
    let held = run_with(&script, &card, Vec::new()).unwrap();
    assert!(held.drawer.holding, "timed for its hold");
    assert_eq!(
        held.drawer.pin.as_ref().map(|p| p.rect),
        p.drawer.pin.as_ref().map(|p| p.rect)
    );
    assert_eq!(held.drawer.reply, Some(record), "it does not press in");
    assert!(held.drawer.prompt.is_none(), "nothing opened yet");
}

/// `view(card)` with forty replies before the card, so the conversation
/// scrolls.
fn long_view(card: AgentCard) -> Event {
    let mut event = view(card);
    if let Event::AgentView {
        sections, changed, ..
    } = &mut event
    {
        let card = sections.pop().expect("the card");
        sections.clear();
        for i in 0..40u32 {
            let text = format!("Reply {i}: a line of the conversation, long enough to wrap once.");
            sections.push(ViewSection {
                index: u64::from(i),
                serial: Serial::new(7, 2 * i + 2, 0),
                shows: Shows::Entries(vec![super::pinned_cards_tests::entry(
                    libflankr_proto::common::Role::Assistant,
                    &text,
                )]),
            });
        }
        let at = sections.len() as u64;
        sections.push(ViewSection { index: at, ..card });
        *changed = sections.iter().map(|s| s.serial).collect();
    }
    event
}

#[test]
fn a_drag_from_a_question_cards_record_scrolls_the_conversation() {
    let card = live(CardKind::Question, "Claude is asking", "Shall I go on?");
    let Some((open, _, s)) = open_view(&card) else {
        return;
    };
    let long = || vec![("OpenAgent", long_view(card.clone()))];
    let settled = format!("{open}frames {SETTLE}\n");
    let p = run_with(&settled, &card, long()).unwrap();
    let record = p.drawer.reply.expect("the card's record");
    let before = p.drawer.agent_offset.expect("the view");
    assert!(before > 400.0, "it scrolls: {before}");
    // Down past the 10 pt slop, then 300 px more, held there.
    let [x, y] = record.center();
    let past = y + 12.0 * s;
    let script = format!(
        "{settled}touch 6 start {x} {y}\nframes 1\ntouch 6 move {x} {past}\nframes 1\n\
         touch 6 move {x} {}\nframes 3\n",
        past + 300.0
    );
    let p = run_with(&script, &card, long()).unwrap();
    let after = p.drawer.agent_offset.unwrap();
    assert!(
        ((before - after) - 300.0).abs() < 1.0,
        "scrolled by the drag after the slop: {before} -> {after}"
    );
    assert!(!p.drawer.holding, "no longer the record's");
    // Released: nothing is sent, nothing opens.
    let p = run_with(
        &format!(
            "{script}touch 6 end {x} {}\nframes {SETTLE}\n",
            past + 300.0
        ),
        &card,
        long(),
    )
    .unwrap();
    for input in ["StartDictation", "StartTextPrompt"] {
        assert_eq!(inputs_with(&p, input), 0, "{:?}", p.inputs);
    }
    assert!(p.drawer.prompt.is_none());
    // Still, the hold and the tap are as before, on the long view too.
    let p = run_with(&format!("{settled}{}", press(record, HOLD)), &card, long()).unwrap();
    assert_eq!(inputs_with(&p, "StartTextPrompt"), 1, "{:?}", p.inputs);
    let p = run_with(&format!("{settled}{}", press(record, 3)), &card, long()).unwrap();
    assert_eq!(inputs_with(&p, "StartDictation"), 1, "{:?}", p.inputs);
}
