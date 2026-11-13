//! The plan policy plugin's device half,
//! `dev.flankr.plugins.policy` (docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md,
//! 2.7, as its 22:31 EST amendment narrows it: the plans and their tasks
//! are managed in the Active tasks view, so this half keeps only the
//! plugin's state and decisions).
//!
//! - **Its popup menu** (the ⋮ item *Plan policy*): running or paused,
//!   with **Pause** or **Resume**; then each session's last decision, with
//!   **Stop** for a session whose continue waits on you.
//! - **The Finished card's line** (2.7 item 2): *Policy: continue, step 4
//!   — waiting for you*, or the `stop`/`ask` word with its reason, on the
//!   card the decision names by its pane and serial.
//! - **The confirmation card's requester** (2.7 item 3): a continue it
//!   asked for reads **Plan policy**.

use std::collections::HashMap;

use flankr_companion::policy::{PolicyDecision, PolicyNotice, PolicySession, PolicyState, methods};
use libcontrols::{DrawContext, PopupItem, PopupRow, Rect};
use libflankr_proto::companion::PluginNotify;
use libmsdf::drawlist::DrawList;
use serde_json::json;

use super::{DevicePlugin, PluginMessage, PluginOut, ViewKind};
use crate::settings::copy;

pub use flankr_companion::policy::NS;

/// The menu item's id, as the daemon half declares it.
pub const ITEM: &str = "policy";
/// The popup's ids.
const PAUSE: &str = "pause";
const STOP: &str = "stop/";
/// The longest reason in the popup, in characters.
const REASON_COLUMNS: usize = 48;

/// Where a continue's confirmation stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Confirmation {
    Waiting,
    Sent,
    Declined,
}

/// The device half.
#[derive(Default)]
pub struct Policy {
    /// `state` as last answered or told; `None` until then.
    state: Option<PolicyState>,
    /// Each pane's last decision.
    decisions: HashMap<String, PolicyDecision>,
    /// The continues it asked for, and how each stands.
    confirms: HashMap<u64, Confirmation>,
}

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }

    fn take(&mut self, decision: PolicyDecision) {
        if let Some(id) = decision.confirm_id {
            self.confirms.entry(id).or_insert(Confirmation::Waiting);
        }
        self.decisions.insert(decision.pane_id.clone(), decision);
    }

    fn take_state(&mut self, state: PolicyState) {
        for s in &state.sessions {
            if let Some(d) = &s.last_decision {
                self.take(d.clone());
            }
        }
        self.state = Some(state);
    }

    fn sessions(&self) -> Vec<PolicySession> {
        let mut out: Vec<PolicySession> = self
            .state
            .as_ref()
            .map(|s| s.sessions.clone())
            .unwrap_or_default();
        for (pane, d) in &self.decisions {
            match out.iter_mut().find(|s| &s.pane_id == pane) {
                Some(s) => s.last_decision = Some(d.clone()),
                None => out.push(PolicySession {
                    pane_id: pane.clone(),
                    agent: d.agent.clone(),
                    project: d.project.clone(),
                    plan: d.plan.clone(),
                    last_decision: Some(d.clone()),
                    ..PolicySession::default()
                }),
            }
        }
        out.sort_by(|a, b| a.pane_id.cmp(&b.pane_id));
        out
    }

    /// The Finished card line for `pane_id`'s last decision: the card's
    /// serial and the line, without its "Policy: " lead.
    pub fn line(&self, pane_id: &str) -> Option<(u64, String)> {
        let d = self
            .decisions
            .iter()
            .find(|(p, _)| same_pane(p, pane_id))
            .map(|(_, d)| d)?;
        let mut line = d.line();
        if let Some(state) = d.confirm_id.and_then(|id| self.confirms.get(&id)) {
            let word = match state {
                Confirmation::Waiting => copy::POLICY_WAITING.text,
                Confirmation::Sent => copy::POLICY_SENT.text,
                Confirmation::Declined => copy::POLICY_DECLINED.text,
            };
            line = format!("{line} \u{2014} {word}");
        }
        Some((d.summary_serial, line))
    }

    /// Whether it asked for the confirmation `confirm_id`.
    pub fn asked_for(&self, confirm_id: u64) -> bool {
        self.confirms.contains_key(&confirm_id)
    }

    /// A confirmation ended, with plan 01's outcome.
    pub fn resolved(&mut self, confirm_id: u64, outcome: &str) {
        if let Some(c) = self.confirms.get_mut(&confirm_id) {
            *c = if outcome == "sent" {
                Confirmation::Sent
            } else {
                Confirmation::Declined
            };
        }
    }

    fn session_row(s: &PolicySession) -> Vec<PopupRow> {
        let mut head = s.agent.clone();
        if !s.project.is_empty() {
            head = format!("{head} \u{b7} {}", s.project);
        }
        let mut rows: Vec<PopupRow> = vec![PopupItem::heading(head).into()];
        if s.blocked {
            rows.push(PopupItem::heading(copy::POLICY_BLOCKED.text).into());
        }
        if let Some(d) = &s.last_decision {
            let mut text = d.line();
            if text.chars().count() > REASON_COLUMNS {
                text = text.chars().take(REASON_COLUMNS - 1).collect::<String>() + "\u{2026}";
            }
            rows.push(PopupItem::heading(text).into());
        }
        let waiting = s
            .last_decision
            .as_ref()
            .and_then(|d| d.confirm_id)
            .is_some();
        if waiting {
            rows.push(
                PopupItem::action(format!("{STOP}{}", s.pane_id), copy::POLICY_STOP.text).into(),
            );
        }
        rows
    }
}

/// Whether two pane ids name the same pane: `w1:p1` and `p1` do.
fn same_pane(a: &str, b: &str) -> bool {
    a == b || a.ends_with(&format!(":{b}")) || b.ends_with(&format!(":{a}"))
}

impl DevicePlugin for Policy {
    fn ns(&self) -> &'static str {
        NS
    }

    fn supports(&self) -> &'static str {
        "0.1"
    }

    /// Opened as a modal by an app that cannot open its popup: the same
    /// state is asked for.
    fn action(&mut self, _item: &str, out: &mut PluginOut) {
        out.call(methods::STATE, json!({}));
    }

    fn message(&mut self, msg: PluginMessage, out: &mut PluginOut) {
        match msg {
            PluginMessage::Result { result: Ok(v), .. } => {
                if let Ok(state) = serde_json::from_value::<PolicyState>(v.clone())
                    && v.get("sessions").is_some()
                {
                    self.take_state(state);
                } else if let Some(paused) = v.get("paused").and_then(|p| p.as_bool()) {
                    self.state.get_or_insert_with(PolicyState::default).paused = paused;
                }
            }
            PluginMessage::Result { result: Err(e), .. } => {
                log::info!("plan policy: {}", e.message);
            }
            PluginMessage::Notify { method, params } => {
                let note = PluginNotify {
                    ns: NS.to_string(),
                    method,
                    params,
                };
                match PolicyNotice::of(&note) {
                    Some(PolicyNotice::Decision(d)) => self.take(d),
                    Some(PolicyNotice::State(s)) => self.take_state(s),
                    // The plans' rows are the Active tasks view's.
                    Some(_) | None => {}
                }
            }
        }
        out.redraw();
    }

    fn height(&self, _view: &ViewKind, _w: f32, _s: f32) -> f32 {
        0.0
    }

    fn draw(
        &self,
        _view: &ViewKind,
        _rect: Rect,
        _s: f32,
        _ctx: &DrawContext,
        _list: &mut DrawList,
    ) {
    }

    fn tap(&mut self, _view: &ViewKind, _x: f32, _y: f32, _out: &mut PluginOut) {}

    fn close(&mut self, _view: &ViewKind) {}

    fn menu_opened(&mut self, item: &str, out: &mut PluginOut) {
        if item == ITEM {
            out.call(methods::STATE, json!({}));
        }
    }

    fn menu(&self, item: &str) -> Vec<PopupRow> {
        if item != ITEM {
            return Vec::new();
        }
        let Some(state) = &self.state else {
            return vec![PopupItem::heading(copy::POLICY_READING.text).into()];
        };
        let (status, toggle) = if state.paused {
            (copy::POLICY_PAUSED.text, copy::POLICY_RESUME.text)
        } else {
            (copy::POLICY_RUNNING.text, copy::POLICY_PAUSE.text)
        };
        let mut rows: Vec<PopupRow> = vec![
            PopupItem::heading(status).into(),
            PopupItem::action(PAUSE, toggle).into(),
            PopupItem::separator().into(),
        ];
        let sessions = self.sessions();
        if sessions.is_empty() {
            rows.push(PopupItem::heading(copy::POLICY_NONE.text).into());
        }
        for s in &sessions {
            rows.extend(Self::session_row(s));
        }
        rows
    }

    fn menu_chosen(&mut self, item: &str, id: &str, out: &mut PluginOut) {
        if item != ITEM {
            return;
        }
        if id == PAUSE {
            let paused = self.state.as_ref().is_some_and(|s| s.paused);
            out.call(methods::PAUSE, json!({ "paused": !paused }));
        } else if let Some(pane) = id.strip_prefix(STOP) {
            out.call(methods::STOP, json!({ "pane_id": pane }));
        }
    }

    fn card_line(&self, pane_id: &str) -> Option<(u64, String)> {
        self.line(pane_id)
    }

    fn confirm_resolved(&mut self, confirm_id: u64, outcome: &str) {
        self.resolved(confirm_id, outcome);
    }

    fn requester(&self, confirm_id: u64) -> Option<&'static str> {
        self.asked_for(confirm_id)
            .then_some(copy::POLICY_REQUESTER.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flankr_companion::policy::notices;
    use serde_json::Value;

    fn notify(p: &mut Policy, method: &str, params: Value) {
        let mut out = PluginOut::new(1);
        p.message(
            PluginMessage::Notify {
                method: method.to_string(),
                params,
            },
            &mut out,
        );
    }

    fn decision(confirm: Option<u64>, decision: &str) -> Value {
        json!({
            "pane_id": "w1:p1", "agent": "claude", "project": "flankr",
            "plan": "docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md",
            "step": 4, "decision": decision, "reason": "Tests fail.",
            "rule": "judgement", "summary_serial": 42, "confirm_id": confirm
        })
    }

    #[test]
    fn the_card_line_follows_the_confirmation() {
        let mut p = Policy::new();
        assert_eq!(p.line("w1:p1"), None);
        notify(&mut p, notices::DECISION, decision(Some(7), "continue"));
        assert_eq!(
            p.line("w1:p1"),
            Some((42, "continue, step 4 \u{2014} waiting for you".to_string()))
        );
        assert_eq!(p.requester(7), Some("Plan policy"));
        assert_eq!(p.requester(8), None);
        p.confirm_resolved(7, "sent");
        assert_eq!(p.line("w1:p1").unwrap().1, "continue, step 4 \u{2014} sent");
        p.confirm_resolved(7, "denied");
        assert_eq!(
            p.line("w1:p1").unwrap().1,
            "continue, step 4 \u{2014} declined"
        );
        notify(&mut p, notices::DECISION, decision(None, "stop"));
        assert_eq!(p.line("w1:p1").unwrap().1, "stop: Tests fail.");
        assert_eq!(p.line("w1:p2"), None);
        assert!(p.line("p1").is_some(), "the short form names it too");
    }

    #[test]
    fn the_popup_pauses_and_stops() {
        let mut p = Policy::new();
        let mut out = PluginOut::new(1);
        p.menu_opened(ITEM, &mut out);
        assert_eq!(out.calls[0].method, "state");
        assert_eq!(p.menu(ITEM).len(), 1, "reading");
        let mut out = PluginOut::new(2);
        p.message(
            PluginMessage::Result {
                id: 1,
                result: Ok(json!({"paused": false, "sessions": [{
                    "pane_id": "w1:p1", "agent": "claude", "project": "flankr",
                    "last_decision": decision(Some(7), "continue"), "blocked": false
                }]})),
            },
            &mut out,
        );
        let rows = p.menu(ITEM);
        assert!(rows.len() >= 5, "{}", rows.len());
        let mut out = PluginOut::new(3);
        p.menu_chosen(ITEM, PAUSE, &mut out);
        assert_eq!(out.calls[0].method, "pause");
        assert_eq!(out.calls[0].params, json!({"paused": true}));
        let mut out = PluginOut::new(4);
        p.menu_chosen(ITEM, "stop/w1:p1", &mut out);
        assert_eq!(out.calls[0].method, "stop");
        assert_eq!(out.calls[0].params, json!({"pane_id": "w1:p1"}));
        // A state notice with the session blocked.
        notify(
            &mut p,
            notices::STATE,
            json!({"paused": true, "sessions": [{"pane_id": "w1:p1", "agent": "claude", "blocked": true, "question": "Which way?"}]}),
        );
        assert!(p.state.as_ref().unwrap().paused);
        assert!(p.state.as_ref().unwrap().sessions[0].blocked);
    }
}
