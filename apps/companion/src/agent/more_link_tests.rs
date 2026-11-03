//! *More*, *Less* and a summary card's dim "more" at the card's bottom
//! right (plan 20261022_03, the L3 amendment of 2026-11-03 14:49 EST):
//! right-aligned to the card's inner right, on the card's last line, its
//! bottom at the card's inner bottom: under the text, and under the
//! action row when the card has one, its tap area sharing no pixel with
//! the buttons; the replay beside "spoken" not that row.

use super::*;
use libflankr_proto::common::{Answer, CardKind};

const S: f32 = 2.0;
const X: f32 = 24.0;
const Y: f32 = 300.0;
const W: f32 = 1000.0;

const LONG: &str = "Should I go on with the next step, which moves the readers onto the \
    record trait, changes every test fixture's loader, and then rebuilds the daemon and \
    the app before the check on the phone, or stop here so you can look first? It also \
    touches the settings screen, the drawer and the pairing flow, each with its own tests, \
    and the release notes for the next build.";

fn card(kind: CardKind) -> AgentCard {
    AgentCard {
        kind,
        live: true,
        at: 0,
        title: "Claude is asking".into(),
        text: LONG.into(),
        confirm_id: (kind == CardKind::Approval).then_some(7),
        about_plan: false,
        links: Vec::new(),
        points: Vec::new(),
        spoken: None,
        answers: Vec::new(),
        choices: None,
    }
}

fn answer(label: &str, style: &str) -> Answer {
    Answer {
        label: label.into(),
        style: style.into(),
        sends: Some(label.into()),
        starts: None,
        command: false,
        answers: true,
    }
}

/// Every card kind that can show the link.
fn kinds() -> Vec<(&'static str, AgentCard)> {
    let finished = AgentCard {
        title: "Finished".into(),
        text: "Written and pushed:\n- src/a.rs\n- src/b.rs\n- src/c.rs\n- src/d.rs".into(),
        points: vec!["The readers moved".into(), "Tests pass".into()],
        spoken: Some("Landed the readers and the tests.".into()),
        ..card(CardKind::Finished)
    };
    vec![
        ("finished summary", finished.clone()),
        (
            "finished, no spoken text",
            AgentCard {
                spoken: None,
                ..finished
            },
        ),
        (
            "broke",
            AgentCard {
                title: "Something broke".into(),
                ..card(CardKind::Broke)
            },
        ),
        ("approval", card(CardKind::Approval)),
        ("yes/no question", card(CardKind::Question)),
        (
            "question with answers",
            AgentCard {
                answers: vec![answer("Go on", "approve"), answer("Stop", "deny")],
                ..card(CardKind::Question)
            },
        ),
        (
            "plan question",
            AgentCard {
                about_plan: true,
                ..card(CardKind::Question)
            },
        ),
        (
            "question with points",
            AgentCard {
                points: vec!["One".into()],
                ..card(CardKind::Question)
            },
        ),
    ]
}

/// The pinned card's form, folded or open, as the pinned layout gives it.
fn form(card: &AgentCard, open: bool) -> CardForm<'static> {
    match (card.is_summary(), open) {
        (false, false) => CardForm::Text {
            max_lines: PIN_LINES,
        },
        (false, true) => CardForm::Text { max_lines: 60 },
        (true, false) => CardForm::Short { context: None },
        (true, true) => CardForm::Expanded {
            context: None,
            max_lines: 60,
            key: 9,
            sound: true,
        },
    }
}

/// The card's inner bottom.
fn inner_bottom(h: f32) -> f32 {
    Y + h - CARD_PAD * S
}

fn is_action(b: &FlowButton) -> bool {
    !matches!(b.hit, AgentHit::Replay(_))
}

fn inner_right() -> f32 {
    X + W - CARD_PAD * S
}

fn link_row<'a>(flow: &'a Flow, label: &str) -> &'a Row {
    let rows: Vec<_> = flow.rows.iter().filter(|r| r.text == label).collect();
    assert_eq!(rows.len(), 1, "one {label}");
    rows[0]
}

/// The link's row at the card's bottom right, below every other row and
/// every button, and its tap area: at least 40 pt square, within the
/// card, around the label, and under the action row.
fn assert_bottom_right(what: &str, flow: &Flow, h: f32, label: &str, area: Option<Rect>) {
    let r = link_row(flow, label);
    let lh = line_h(r.px);
    let right = r.x + label.len() as f32 * r.px * 0.5;
    assert!((right - inner_right()).abs() < 0.5, "{what}: right {right}");
    let bottom = inner_bottom(h);
    assert!(
        (r.y + lh - bottom).abs() < 0.5,
        "{what}: bottom {} not {bottom}",
        r.y + lh
    );
    for o in flow.rows.iter().filter(|o| o.text != label) {
        assert!(
            o.y + line_h(o.px) <= r.y + 0.5,
            "{what}: {:?} under it",
            o.text
        );
    }
    for b in &flow.buttons {
        assert!(
            b.rect.y + b.rect.h <= r.y + 0.5,
            "{what}: {:?} under it",
            b.hit
        );
    }
    if let Some(a) = area {
        assert!(a.w >= 40.0 * S - 0.01, "{what}: tap width {}", a.w);
        assert!(a.h >= 40.0 * S - 0.01, "{what}: tap height {}", a.h);
        for b in flow.buttons.iter().filter(|b| is_action(b)) {
            assert!(
                a.y >= b.rect.y + b.rect.h - 0.01,
                "{what}: shares pixels with {:?}",
                b.hit
            );
        }
        assert!(
            a.x + a.w >= right && a.x <= r.x,
            "{what}: tap area across it"
        );
        assert!(
            a.y >= Y - 0.01 && a.y + a.h <= Y + h + 0.01,
            "{what}: in the card"
        );
        assert!(
            a.y <= r.y && a.y + a.h >= r.y + lh - 0.01,
            "{what}: tap area over it"
        );
    }
}

fn lay(card: &AgentCard, form: CardForm) -> (Flow, f32) {
    let mut flow = Flow::default();
    let mut targets = Vec::new();
    let h = card_block(card, X, Y, W, S, form, &mut targets, &mut flow);
    (flow, h)
}

#[test]
fn pinned_more_and_less_sit_at_the_cards_bottom_right() {
    for (what, card) in kinds() {
        for open in [false, true] {
            let label = if open { PIN_LESS } else { PIN_MORE };
            let (mut flow, h) = lay(&card, form(&card, open));
            let (h, area) = add_more_link(&mut flow, (0, 0, 0), X, Y, W, h, label, S);
            let what = format!("{what}, {}", if open { "open" } else { "folded" });
            assert_bottom_right(&what, &flow, h, label, Some(area));
        }
    }
}

#[test]
fn a_summary_cards_more_sits_at_the_cards_bottom_right() {
    let cut = |c: &AgentCard| short_form(c, None, 40).1;
    let kinds: Vec<_> = kinds()
        .into_iter()
        .filter(|(_, c)| c.is_summary() && cut(c))
        .collect();
    assert_eq!(kinds.len(), 2, "a Finished card and a question");
    for (what, card) in kinds {
        let (flow, h) = lay(&card, CardForm::Short { context: None });
        assert_bottom_right(what, &flow, h, MORE, None);
        assert_eq!(link_row(&flow, MORE).color, theme().dim);
    }
}

#[test]
fn less_on_an_open_summary_card_leaves_the_replay_beside_spoken() {
    let (_, card) = kinds().remove(0);
    let (before, h) = lay(&card, form(&card, true));
    let replay = |f: &Flow| {
        f.buttons
            .iter()
            .find(|b| matches!(b.hit, AgentHit::Replay(_)))
            .map(|b| b.rect)
            .expect("the replay")
    };
    let mut flow = before.clone();
    let (h2, _) = add_more_link(&mut flow, (0, 0, 0), X, Y, W, h, PIN_LESS, S);
    assert_eq!(replay(&flow), replay(&before), "the replay stays");
    let spoken = flow.rows.iter().find(|r| r.text == SPOKEN).unwrap();
    let r = replay(&flow);
    let mid = r.y + r.h * 0.5;
    assert!(
        mid > spoken.y && mid < spoken.y + line_h(spoken.px),
        "beside spoken"
    );
    // Less is under the spoken text, at the card's inner bottom.
    let last = before.rows.last().unwrap();
    let less = link_row(&flow, PIN_LESS);
    assert!(less.y >= last.y + line_h(last.px) - 0.5);
    assert!((less.y + line_h(less.px) - inner_bottom(h2)).abs() < 0.5);
}
