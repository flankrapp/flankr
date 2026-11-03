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
