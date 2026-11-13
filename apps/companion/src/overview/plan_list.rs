//! The Active tasks view (docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md,
//! 2.3 to 2.5, with its amendments): the home screen's list of plans, in
//! place of the card grid while *Home view* is *Active tasks*.
//!
//! - **A plan's header**, 2 lines, tap to fold: its title and waiting
//!   marks (amber, confirmations; blue, questions), and, for a task (a
//!   plan Build was chosen on), the right-side glyph that turns its
//!   autocontinue on and off; then, dim, its status, step, agents,
//!   subagents and the policy's last word, or *not armed* in amber.
//! - **An agent line** per linked pane, 2 lines: its name and worktree;
//!   its turn, running subagents and the policy line; one line per
//!   running subagent. A tap opens its agent view.
//! - **Its events** under it, oldest first, at most [`EVENTS_MAX`]: its
//!   open question (Yes / No), its confirmations (Approve / Deny), and the
//!   policy's decisions (a waiting `continue`: Approve / Deny; `ask`:
//!   Continue and Open; `stop`: Open). Closed events older than
//!   [`CLOSED_KEPT`] go.
//! - **Not on a plan** last; and with the plugin off or missing, one line
//!   at the top and every agent there.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use flankr_companion::policy::{
    PlanGroup, PolicyAgentLink, PolicyDecision, PolicyOverview, PolicyPlanGroup,
};
use libcontrols::{DrawContext, Rect};
use libflankr_proto::common::{AgentInfo, Turn};
use libmorph::TouchPhase;
use libmsdf::drawlist::DrawList;

use crate::agent::{fill, line_h};
use crate::controls::scroll::{Scroll, Touched};
use crate::settings::copy;
use crate::theme::theme;

/// The most event rows under one agent.
pub const EVENTS_MAX: usize = 6;
/// How long a closed event stays.
pub const CLOSED_KEPT: Duration = Duration::from_secs(30 * 60);

/// Heights and sizes, in points (2.3 item 5).
const HEADER_H: f32 = 40.0;
const AGENT_H: f32 = 44.0;
const CHILD_H: f32 = 16.0;
const EVENT_H: f32 = 52.0;
const BUTTON_H: f32 = 36.0;
const BUTTON_W: f32 = 96.0;
const TITLE_PT: f32 = 15.0;
const LINE_PT: f32 = 13.0;
const DIM_PT: f32 = 11.0;
const PAD: f32 = 12.0;
const INDENT: f32 = 16.0;
const GLYPH_W: f32 = 72.0;

/// What a tap in the list asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListHit {
    /// Fold or unfold a plan's group.
    Fold(String),
    /// The task's glyph: autocontinue on (`true`) or off.
    Autocontinue { path: String, on: bool },
    /// Open the agent view on this pane.
    Agent(String),
    /// A question's Yes or No.
    Answer { pane_id: String, yes: bool },
    /// A confirmation's Approve or Deny.
    Confirm { confirm_id: u64, approve: bool },
    /// An `ask` decision's Continue: `continue` sent to the pane.
    Continue(String),
    /// *Turn on Plan policy*: Settings.
    Settings,
}

/// Why the list has no plans of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// The plugin answered: groups by plan.
    #[default]
    ByPlan,
    /// The plugin is off (or its call failed).
    PluginOff,
    /// This flankr has no plan policy plugin.
    OlderFlankr,
}

/// One event of a pane, as kept.
#[derive(Clone, Debug, PartialEq)]
enum EventKind {
    Confirmation {
        confirm_id: u64,
        command: String,
        requester: String,
        outcome: Option<String>,
    },
    Decision(PolicyDecision),
}

#[derive(Clone, Debug, PartialEq)]
struct Event {
    kind: EventKind,
    /// "HH:MM", from the daemon's time when it gave one.
    time: Option<String>,
    /// From the daemon (`false`: the phone's receipt).
    daemon_time: bool,
    /// When it came, on the app's clock.
    came: Duration,
    /// When it closed, on the app's clock.
    closed: Option<Duration>,
}

impl Event {
    fn open(&self, confirms: &HashMap<u64, Option<String>>) -> bool {
        match &self.kind {
            EventKind::Confirmation { outcome, .. } => outcome.is_none(),
            EventKind::Decision(d) => d
                .confirm_id
                .is_some_and(|id| confirms.get(&id).is_some_and(Option::is_none)),
        }
    }
}

/// "HH:MM" of an RFC 3339 time.
pub fn clock(ts: &str) -> Option<String> {
    let t = ts.get(11..16)?;
    (t.as_bytes()[2] == b':' && t.chars().filter(char::is_ascii_digit).count() == 4)
        .then(|| t.to_string())
}

/// One thing laid out, in content coordinates.
#[derive(Clone, Debug, PartialEq)]
struct Item {
    rect: Rect,
    texts: Vec<(String, [f32; 2], f32, [f32; 4])>,
    fills: Vec<(Rect, [f32; 4], f32)>,
    hits: Vec<(Rect, ListHit)>,
}

/// The list.
pub struct PlanList {
    viewport: Rect,
    s: f32,
    scroll: Scroll,
    grouping: Grouping,
    overview: Option<PolicyOverview>,
    agents: Vec<AgentInfo>,
    folded: HashSet<String>,
    /// By pane.
    events: HashMap<String, Vec<Event>>,
    /// Each confirmation seen, and its outcome once resolved.
    confirms: HashMap<u64, Option<String>>,
    /// The pane each confirmation is for.
    confirm_panes: HashMap<u64, String>,
    /// Buttons pressed, dim until their event changes.
    pressed: HashSet<String>,
    items: Vec<Item>,
    height: f32,
    now: Duration,
    dirty: bool,
}

impl Default for PlanList {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanList {
    pub fn new() -> Self {
        PlanList {
            viewport: Rect::new(0.0, 0.0, 0.0, 0.0),
            s: 1.0,
            scroll: Scroll::new(false),
            grouping: Grouping::PluginOff,
            overview: None,
            agents: Vec::new(),
            folded: HashSet::new(),
            events: HashMap::new(),
            confirms: HashMap::new(),
            confirm_panes: HashMap::new(),
            pressed: HashSet::new(),
            items: Vec::new(),
            height: 0.0,
            now: Duration::ZERO,
            dirty: true,
        }
    }

    pub fn set_viewport(&mut self, viewport: Rect, s: f32) {
        if viewport != self.viewport || s != self.s {
            self.viewport = viewport;
            self.s = s;
            self.relayout();
        }
    }

    pub fn viewport(&self) -> Rect {
        self.viewport
    }

    /// The app's clock, for the events' ages.
    pub fn set_now(&mut self, now: Duration) {
        self.now = now;
    }

    /// The plugin's answer, or why there is none.
    pub fn set_overview(&mut self, overview: Option<PolicyOverview>, grouping: Grouping) {
        if overview != self.overview || grouping != self.grouping {
            self.overview = overview;
            self.grouping = grouping;
            self.relayout();
        }
    }

    pub fn grouping(&self) -> Grouping {
        self.grouping
    }

    pub fn set_agents(&mut self, agents: &[AgentInfo]) {
        if agents != self.agents.as_slice() {
            self.agents = agents.to_vec();
            self.relayout();
        }
    }

    /// A confirmation asked for `pane_id`.
    pub fn confirm_opened(
        &mut self,
        pane_id: &str,
        confirm_id: u64,
        command: &str,
        requester: &str,
        at: Option<&str>,
    ) {
        if self.confirms.contains_key(&confirm_id) {
            return;
        }
        self.confirms.insert(confirm_id, None);
        self.confirm_panes.insert(confirm_id, pane_id.to_string());
        let time = at.and_then(clock);
        let daemon_time = time.is_some();
        self.push(
            pane_id,
            Event {
                kind: EventKind::Confirmation {
                    confirm_id,
                    command: command.to_string(),
                    requester: requester.to_string(),
                    outcome: None,
                },
                time,
                daemon_time,
                came: self.now,
                closed: None,
            },
        );
    }

    /// A confirmation ended.
    pub fn confirm_resolved(&mut self, confirm_id: u64, outcome: &str) {
        self.confirms.insert(confirm_id, Some(outcome.to_string()));
        let now = self.now;
        for list in self.events.values_mut() {
            for e in list.iter_mut() {
                let ours = match &mut e.kind {
                    EventKind::Confirmation {
                        confirm_id: id,
                        outcome: o,
                        ..
                    } if *id == confirm_id => {
                        *o = Some(outcome.to_string());
                        true
                    }
                    EventKind::Decision(d) => d.confirm_id == Some(confirm_id),
                    _ => false,
                };
                if ours && e.closed.is_none() {
                    e.closed = Some(now);
                }
            }
        }
        self.pressed
            .retain(|k| !k.ends_with(&format!("/{confirm_id}")));
        self.relayout();
    }

    /// A policy decision; one per card, the newest replacing its serial's.
    pub fn decision(&mut self, d: PolicyDecision) {
        let pane = d.pane_id.clone();
        let time = clock(&d.at);
        if let Some(list) = self.events.get_mut(&pane) {
            list.retain(|e| !matches!(&e.kind, EventKind::Decision(o) if o.summary_serial == d.summary_serial));
        }
        let open = d
            .confirm_id
            .is_some_and(|id| self.confirms.get(&id).is_none_or(Option::is_none));
        if let Some(id) = d.confirm_id {
            self.confirms.entry(id).or_insert(None);
            self.confirm_panes.insert(id, pane.clone());
        }
        let now = self.now;
        self.push(
            &pane,
            Event {
                kind: EventKind::Decision(d),
                daemon_time: time.is_some(),
                time,
                came: now,
                closed: (!open).then_some(now),
            },
        );
    }

    fn push(&mut self, pane: &str, e: Event) {
        let list = self.events.entry(pane.to_string()).or_default();
        list.push(e);
        let now = self.now;
        list.retain(|e| e.closed.is_none_or(|c| now.saturating_sub(c) < CLOSED_KEPT));
        let confirms = &self.confirms;
        while list.len() > EVENTS_MAX {
            match list.iter().position(|e| !e.open(confirms)) {
                Some(i) => {
                    list.remove(i);
                }
                None => break,
            }
        }
        self.relayout();
    }

    /// Notes a button pressed: it is dim until its event changes.
    pub fn pressed(&mut self, hit: &ListHit) {
        if let Some(key) = press_key(hit) {
            self.pressed.insert(key);
            self.relayout();
        }
    }

    /// The groups to draw: the plugin's joined to the agents, or one
    /// **Not on a plan** group.
    fn groups(&self) -> Vec<PlanGroup> {
        match (&self.overview, self.grouping) {
            (Some(o), Grouping::ByPlan) => o.join(&self.agents),
            _ => PolicyOverview::default().join(&self.agents),
        }
    }

    fn relayout(&mut self) {
        self.dirty = true;
        let s = self.s;
        let w = self.viewport.w;
        let mut y = 0.0;
        let mut items = Vec::new();
        if self.grouping != Grouping::ByPlan {
            let text = if self.grouping == Grouping::OlderFlankr {
                copy::TASKS_OLDER_FLANKR.text
            } else {
                copy::TASKS_TURN_ON.text
            };
            let h = 32.0 * s;
            items.push(Item {
                rect: Rect::new(0.0, y, w, h),
                texts: vec![(
                    text.to_string(),
                    [PAD * s, y + 8.0 * s],
                    LINE_PT * s,
                    theme().accent,
                )],
                fills: Vec::new(),
                hits: vec![(Rect::new(0.0, y, w, h), ListHit::Settings)],
            });
            y += h;
        }
        let groups = self.groups();
        let linked = groups.iter().filter(|g| g.plan.is_some()).count();
        if self.grouping == Grouping::ByPlan && linked == 0 {
            let h = 28.0 * s;
            items.push(Item {
                rect: Rect::new(0.0, y, w, h),
                texts: vec![(
                    copy::TASKS_NONE.text.to_string(),
                    [PAD * s, y + 6.0 * s],
                    LINE_PT * s,
                    theme().dim,
                )],
                fills: Vec::new(),
                hits: Vec::new(),
            });
            y += h;
        }
        for g in &groups {
            if g.plan.is_none() && g.agents.is_empty() {
                continue;
            }
            let item = self.header(g, y, w);
            y += item.rect.h;
            items.push(item);
            let key = g.plan.as_ref().map_or(String::new(), |p| p.path.clone());
            if self.folded.contains(&key) {
                continue;
            }
            for (agent, link) in &g.agents {
                let item = self.agent_line(agent, link.as_ref(), y, w);
                y += item.rect.h;
                items.push(item);
                for item in self.event_rows(agent, y, w) {
                    y += item.rect.h;
                    items.push(item);
                }
            }
        }
        self.items = items;
        self.height = y;
        self.scroll.set_extent(y, self.viewport.h);
    }

    fn header(&self, g: &PlanGroup, y: f32, w: f32) -> Item {
        let s = self.s;
        let h = HEADER_H * s;
        let rect = Rect::new(0.0, y, w, h);
        let mut texts = Vec::new();
        let mut fills = vec![(
            Rect::new(0.0, y + h - 1.0 * s, w, 1.0 * s),
            theme().band,
            0.0,
        )];
        let mut hits = Vec::new();
        let key = g.plan.as_ref().map_or(String::new(), |p| p.path.clone());
        let glyph = g.plan.as_ref().filter(|p| p.task).map(|p| {
            let r = Rect::new(w - (GLYPH_W + PAD) * s, y + 6.0 * s, GLYPH_W * s, 28.0 * s);
            (r, p.autocontinue, p.path.clone())
        });
        let title_w =
            w - 2.0 * PAD * s - glyph.as_ref().map_or(0.0, |(r, _, _)| r.w + 8.0 * s) - 48.0 * s;
        let title = match &g.plan {
            Some(p) => p
                .title
                .clone()
                .unwrap_or_else(|| p.path.rsplit('/').next().unwrap_or("").to_string()),
            None => copy::TASKS_NOT_ON_A_PLAN.text.to_string(),
        };
        texts.push((
            ellipsis(&title, title_w, TITLE_PT * s),
            [PAD * s, y + 4.0 * s],
            TITLE_PT * s,
            theme().fg,
        ));
        // The waiting marks: amber confirmations, blue questions.
        let (confirms, questions) = self.waiting(g);
        let mut mx = w - PAD * s - glyph.as_ref().map_or(0.0, |(r, _, _)| r.w + 8.0 * s);
        for (n, colour) in [(questions, theme().question), (confirms, theme().accent)] {
            if n > 0 {
                mx -= 22.0 * s;
                fills.push((
                    Rect::new(mx, y + 8.0 * s, 18.0 * s, 18.0 * s),
                    colour,
                    9.0 * s,
                ));
                texts.push((
                    n.to_string(),
                    [mx + 5.0 * s, y + 9.0 * s],
                    DIM_PT * s,
                    theme().bg,
                ));
            }
        }
        if let Some(p) = &g.plan {
            texts.push((
                header_line(p, g),
                [PAD * s, y + 22.0 * s],
                DIM_PT * s,
                header_colour(p),
            ));
        }
        if let Some((r, on, path)) = glyph {
            fills.push((r, if on { theme().green } else { theme().band }, 14.0 * s));
            let label = if on {
                copy::TASKS_AUTO_ON.text
            } else {
                copy::TASKS_AUTO_OFF.text
            };
            texts.push((
                label.to_string(),
                [r.x + 10.0 * s, r.y + 6.0 * s],
                DIM_PT * s,
                theme().fg,
            ));
            hits.push((r, ListHit::Autocontinue { path, on: !on }));
        }
        hits.push((rect, ListHit::Fold(key)));
        Item {
            rect,
            texts,
            fills,
            hits,
        }
    }

    fn waiting(&self, g: &PlanGroup) -> (usize, usize) {
        let mut confirms = 0;
        let mut questions = 0;
        for (a, _) in &g.agents {
            confirms += self
                .confirm_panes
                .iter()
                .filter(|(id, p)| {
                    *p == &a.pane_id && self.confirms.get(id).is_some_and(Option::is_none)
                })
                .count();
            if question_of(a).is_some() {
                questions += 1;
            }
        }
        (confirms, questions)
    }

    fn agent_line(&self, a: &AgentInfo, link: Option<&PolicyAgentLink>, y: f32, w: f32) -> Item {
        let s = self.s;
        let running: Vec<String> = a
            .children
            .iter()
            .filter(|c| !c.finished)
            .map(|c| c.short.clone())
            .collect();
        let h = (AGENT_H + CHILD_H * running.len() as f32) * s;
        let rect = Rect::new(0.0, y, w, h);
        let x = (PAD + INDENT) * s;
        let mut name = a.name.clone().unwrap_or_else(|| a.agent.clone());
        let branch = link
            .and_then(|l| l.worktree.as_ref())
            .and_then(|wt| wt.branch.clone().or(wt.label.clone()))
            .or_else(|| {
                a.worktree
                    .as_ref()
                    .and_then(|wt| wt.branch.clone().or(wt.label.clone()))
            });
        if let Some(b) = branch {
            name = format!("{name} \u{b7} {b}");
        }
        let mut texts = vec![(
            ellipsis(&name, w - x - PAD * s, LINE_PT * s),
            [x, y + 4.0 * s],
            LINE_PT * s,
            theme().fg,
        )];
        let mut second = turn_word(a);
        if let Some(n) = a.subagents_running.filter(|n| *n > 0) {
            second.push_str(&format!(" \u{b7} {n} running"));
        }
        if let Some(d) = link.and_then(|l| l.last_decision.as_ref()) {
            second.push_str(&format!(
                " \u{b7} {}{}",
                crate::agent::POLICY_LEAD,
                self.decision_line(d)
            ));
        }
        texts.push((
            ellipsis(&second, w - x - PAD * s, DIM_PT * s),
            [x, y + 22.0 * s],
            DIM_PT * s,
            theme().dim,
        ));
        for (i, c) in running.iter().enumerate() {
            texts.push((
                c.clone(),
                [
                    x + INDENT * s,
                    y + (AGENT_H + CHILD_H * i as f32) * s - 4.0 * s,
                ],
                DIM_PT * s,
                theme().dim,
            ));
        }
        let mut fills = Vec::new();
        if a.focused {
            fills.push((
                Rect::new(PAD * s, y + 4.0 * s, 3.0 * s, h - 8.0 * s),
                theme().accent,
                1.5 * s,
            ));
        }
        Item {
            rect,
            texts,
            fills,
            hits: vec![(rect, ListHit::Agent(a.pane_id.clone()))],
        }
    }

    /// A decision's words, with its confirmation's state for a continue.
    fn decision_line(&self, d: &PolicyDecision) -> String {
        let mut line = d.line();
        if let Some(id) = d.confirm_id {
            let word = match self.confirms.get(&id) {
                Some(Some(o)) if o == "sent" => copy::POLICY_SENT.text,
                Some(Some(_)) => copy::POLICY_DECLINED.text,
                _ => copy::POLICY_WAITING.text,
            };
            line = format!("{line} \u{2014} {word}");
        }
        line
    }

    fn event_rows(&self, a: &AgentInfo, mut y: f32, w: f32) -> Vec<Item> {
        let s = self.s;
        let x = (PAD + 2.0 * INDENT) * s;
        let mut rows: Vec<RowSpec> = Vec::new();
        let mut kept: Vec<&Event> = self
            .events
            .get(&a.pane_id)
            .map(|l| l.iter().collect())
            .unwrap_or_default();
        kept.sort_by_key(|e| e.came);
        for e in &kept {
            let open = e.open(&self.confirms);
            let (label, colour, buttons) = match &e.kind {
                EventKind::Confirmation {
                    confirm_id,
                    command,
                    requester,
                    outcome,
                } => {
                    let who = if requester == flankr_companion::policy::PRINCIPAL_ID {
                        copy::POLICY_REQUESTER.text.to_string()
                    } else {
                        requester.clone()
                    };
                    let mut text = format!("{} \u{b7} {who}: {command}", copy::TASKS_APPROVAL.text);
                    if let Some(o) = outcome {
                        text.push_str(&format!(" \u{2014} {o}"));
                    }
                    let b = if open {
                        vec![
                            (
                                copy::TASKS_APPROVE.text.to_string(),
                                ListHit::Confirm {
                                    confirm_id: *confirm_id,
                                    approve: true,
                                },
                            ),
                            (
                                copy::TASKS_DENY.text.to_string(),
                                ListHit::Confirm {
                                    confirm_id: *confirm_id,
                                    approve: false,
                                },
                            ),
                        ]
                    } else {
                        Vec::new()
                    };
                    (text, theme().accent, b)
                }
                EventKind::Decision(d) => {
                    let text = format!("{}{}", crate::agent::POLICY_LEAD, self.decision_line(d));
                    let b = match (d.decision.as_str(), d.confirm_id) {
                        ("continue", Some(id)) if open => vec![
                            (
                                copy::TASKS_APPROVE.text.to_string(),
                                ListHit::Confirm {
                                    confirm_id: id,
                                    approve: true,
                                },
                            ),
                            (
                                copy::TASKS_DENY.text.to_string(),
                                ListHit::Confirm {
                                    confirm_id: id,
                                    approve: false,
                                },
                            ),
                        ],
                        ("ask", _) => vec![
                            (
                                copy::TASKS_CONTINUE.text.to_string(),
                                ListHit::Continue(a.pane_id.clone()),
                            ),
                            (
                                copy::TASKS_OPEN.text.to_string(),
                                ListHit::Agent(a.pane_id.clone()),
                            ),
                        ],
                        ("stop", _) => vec![(
                            copy::TASKS_OPEN.text.to_string(),
                            ListHit::Agent(a.pane_id.clone()),
                        )],
                        _ => Vec::new(),
                    };
                    (text, if open { theme().fg } else { theme().dim }, b)
                }
            };
            rows.push((e.time.clone(), e.daemon_time, label, colour, buttons));
        }
        // The open question: its time is its own; it sorts by it among
        // the rest only as it came, so it goes last, as the newest.
        if let Some(q) = question_of(a) {
            rows.push((
                None,
                true,
                format!("{} \u{b7} {q}", copy::TASKS_ASKING.text),
                theme().question,
                vec![
                    (
                        copy::TASKS_YES.text.to_string(),
                        ListHit::Answer {
                            pane_id: a.pane_id.clone(),
                            yes: true,
                        },
                    ),
                    (
                        copy::TASKS_NO.text.to_string(),
                        ListHit::Answer {
                            pane_id: a.pane_id.clone(),
                            yes: false,
                        },
                    ),
                ],
            ));
        }
        let mut out = Vec::new();
        for (time, daemon_time, text, colour, buttons) in rows {
            let h = if buttons.is_empty() {
                24.0 * s
            } else {
                EVENT_H * s + 4.0 * s
            };
            let rect = Rect::new(0.0, y, w, h);
            let mut texts = Vec::new();
            let lead = match &time {
                Some(t) => {
                    texts.push((
                        t.clone(),
                        [x, y + 4.0 * s],
                        DIM_PT * s,
                        if daemon_time {
                            theme().dim
                        } else {
                            theme().band
                        },
                    ));
                    40.0 * s
                }
                None => 0.0,
            };
            texts.push((
                ellipsis(&text, w - x - lead - PAD * s, DIM_PT * s),
                [x + lead, y + 4.0 * s],
                DIM_PT * s,
                colour,
            ));
            let mut fills = Vec::new();
            let mut hits = Vec::new();
            let mut bx = x;
            for (label, hit) in buttons {
                let r = Rect::new(bx, y + 18.0 * s, BUTTON_W * s, BUTTON_H * s * 0.8);
                let dim = press_key(&hit).is_some_and(|k| self.pressed.contains(&k));
                fills.push((r, if dim { theme().band } else { theme().card }, 8.0 * s));
                texts.push((
                    label,
                    [r.x + 12.0 * s, r.y + 7.0 * s],
                    LINE_PT * s,
                    if dim { theme().dim } else { theme().fg },
                ));
                hits.push((r, hit));
                bx += (BUTTON_W + 8.0) * s;
            }
            out.push(Item {
                rect,
                texts,
                fills,
                hits,
            });
            y += h;
        }
        out
    }

    /// The hit at `(x, y)` on the screen: a button before its row.
    pub fn hit_at(&self, x: f32, y: f32) -> Option<ListHit> {
        if !self.viewport.contains([x, y]) {
            return None;
        }
        let cy = y - self.viewport.y + self.scroll.offset();
        for item in &self.items {
            if item.rect.contains([x, cy]) {
                // The smallest area first: a button over its row.
                let mut hits: Vec<&(Rect, ListHit)> = item
                    .hits
                    .iter()
                    .filter(|(r, _)| r.contains([x, cy]))
                    .collect();
                hits.sort_by(|a, b| (a.0.w * a.0.h).total_cmp(&(b.0.w * b.0.h)));
                return hits.first().map(|(_, h)| h.clone());
            }
        }
        None
    }

    /// A touch: a drag scrolls; a tap gives its hit (and folds a group
    /// itself).
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        x: f32,
        y: f32,
        now: Duration,
    ) -> Option<ListHit> {
        match self.scroll.touch(id, phase, x, y, now, self.s) {
            Touched::Scrolled => {
                self.dirty = true;
                None
            }
            Touched::Tap => {
                let hit = self.hit_at(x, y)?;
                if let ListHit::Fold(key) = &hit {
                    if !self.folded.remove(key) {
                        self.folded.insert(key.clone());
                    }
                    self.relayout();
                }
                Some(hit)
            }
            Touched::Nothing => None,
        }
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        let moved = self.scroll.tick(dt, self.s);
        if moved {
            self.dirty = true;
        }
        moved
    }

    pub fn draw(&self, list: &mut DrawList, ctx: &DrawContext) {
        let top = self.viewport.y - self.scroll.offset();
        let (vy, vh) = (self.viewport.y, self.viewport.h);
        for item in &self.items {
            let r = item.rect;
            if r.y + top + r.h < vy || r.y + top > vy + vh {
                continue;
            }
            for (f, colour, radius) in &item.fills {
                fill(list, Rect::new(f.x, f.y + top, f.w, f.h), *colour, *radius);
            }
            for (text, at, px, colour) in &item.texts {
                let ty = at[1] + top;
                if ty < vy - line_h(*px) * 0.5 || ty > vy + vh {
                    continue;
                }
                ctx.draw_text(list, text, [at[0], ty], *px, *colour);
            }
        }
    }

    /// The texts laid out, top to bottom (tests).
    pub fn texts(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| i.texts.iter().map(|t| t.0.clone()))
            .collect()
    }

    /// Where each hit is, on the screen (tests).
    pub fn hits(&self) -> Vec<(Rect, ListHit)> {
        let top = self.viewport.y - self.scroll.offset();
        self.items
            .iter()
            .flat_map(|i| {
                i.hits
                    .iter()
                    .map(move |(r, h)| (Rect::new(r.x, r.y + top, r.w, r.h), h.clone()))
            })
            .collect()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// An event row before it is placed: its time, whether the daemon gave
/// it, its text and colour, and its buttons.
type RowSpec = (
    Option<String>,
    bool,
    String,
    [f32; 4],
    Vec<(String, ListHit)>,
);

fn press_key(hit: &ListHit) -> Option<String> {
    match hit {
        ListHit::Confirm { confirm_id, .. } => Some(format!("confirm/{confirm_id}")),
        ListHit::Answer { pane_id, .. } => Some(format!("answer/{pane_id}")),
        ListHit::Continue(pane) => Some(format!("continue/{pane}")),
        _ => None,
    }
}

/// The pane's open question, from its session.
fn question_of(a: &AgentInfo) -> Option<String> {
    if let Some(Turn::Asking { question }) = &a.turn {
        return Some(question.clone());
    }
    a.session
        .as_ref()
        .and_then(|s| s.pending_question.as_ref())
        .map(|q| q.text.clone())
}

/// The turn word as the card shows it.
fn turn_word(a: &AgentInfo) -> String {
    match &a.turn {
        Some(Turn::Asking { .. }) => copy::TASKS_ASKING.text.to_string(),
        Some(Turn::Stopped { .. }) => "Stopped".to_string(),
        Some(Turn::Working { line }) if !line.trim().is_empty() => {
            line.lines().next().unwrap_or("").trim().to_string()
        }
        Some(Turn::Working { .. }) => "Working".to_string(),
        None => a.status.clone(),
    }
}

/// A header's dim line (2.3 item 2): status · step N of M · n agents ·
/// m subagents · policy: <decision> <time>, or *not armed*.
pub fn header_line(p: &PolicyPlanGroup, g: &PlanGroup) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(st) = &p.status {
        parts.push(st.clone());
    }
    match (p.current_step, p.steps_total) {
        (Some(n), Some(t)) => parts.push(format!("step {n} of {t}")),
        (None, Some(t)) => parts.push(format!("{t} steps")),
        _ => {}
    }
    let n = g.agents.len();
    parts.push(format!("{n} agent{}", if n == 1 { "" } else { "s" }));
    let m: u32 = g
        .agents
        .iter()
        .filter_map(|(a, _)| a.subagents_running)
        .sum();
    if m > 0 {
        parts.push(format!("{m} subagents"));
    }
    if !p.armed {
        parts.push(copy::TASKS_NOT_ARMED.text.to_string());
    } else {
        let newest = g
            .agents
            .iter()
            .filter_map(|(_, l)| l.as_ref()?.last_decision.as_ref())
            .max_by(|a, b| a.at.cmp(&b.at));
        if let Some(d) = newest {
            let mut w = format!("policy: {}", d.decision);
            if let Some(t) = clock(&d.at) {
                w.push_str(&format!(" {t}"));
            }
            parts.push(w);
        }
    }
    parts.join(" \u{b7} ")
}

fn header_colour(p: &PolicyPlanGroup) -> [f32; 4] {
    if p.armed { theme().dim } else { theme().accent }
}

/// `text` cut with an ellipsis to fit `w` at `px` (an estimate of the
/// face's advance, as the agent view's columns).
fn ellipsis(text: &str, w: f32, px: f32) -> String {
    let cols = crate::agent::columns(w.max(px), px).max(4);
    if text.chars().count() <= cols {
        return text.to_string();
    }
    let kept: String = text.chars().take(cols - 1).collect();
    format!("{}\u{2026}", kept.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agent(pane: &str, extra: serde_json::Value) -> AgentInfo {
        let mut v = json!({"pane_id": pane, "agent": "claude", "status": "idle", "focused": false, "name": format!("Claude {pane}")});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).unwrap()
    }

    fn overview() -> PolicyOverview {
        serde_json::from_value(json!({
            "plans": [
                {"path": "docs/design/20261112_02_A_PLAN.md", "title": "Plan policy agent",
                 "status": "In implementation", "armed": true, "task": true, "autocontinue": true,
                 "steps_total": 9, "current_step": 3,
                 "agents": [{"pane_id": "p1", "linked_by": "pinned",
                             "worktree": {"path": "/r/w", "branch": "plan/20261112_02"},
                             "last_decision": {"pane_id": "p1", "decision": "continue", "step": 4,
                                               "reason": "Done.", "confirm_id": 7,
                                               "summary_serial": 9, "at": "2026-11-12T21:30:00-05:00"}}]},
                {"path": "docs/design/20261112_03_B_PLAN.md", "title": "Active plans",
                 "status": "Draft", "armed": false, "task": false, "steps_total": 5,
                 "agents": [{"pane_id": "p2", "linked_by": "gemini"}]}
            ],
            "unlinked": ["p3"], "paused": false
        }))
        .unwrap()
    }

    fn list() -> PlanList {
        let mut l = PlanList::new();
        l.set_viewport(Rect::new(0.0, 100.0, 1080.0, 1600.0), 2.0);
        l.set_agents(&[
            agent("p1", json!({"subagents_running": 2, "focused": true})),
            agent(
                "p2",
                json!({"turn": {"state": "asking", "question": "Shall I go on?"}}),
            ),
            agent("p3", json!({})),
            agent("p4", json!({"agent": "shell"})),
        ]);
        l.set_overview(Some(overview()), Grouping::ByPlan);
        l
    }

    #[test]
    fn headers_say_the_plan_at_a_glance() {
        let l = list();
        let t = l.texts();
        assert!(t.contains(&"Plan policy agent".to_string()), "{t:?}");
        assert!(t.contains(&"In implementation \u{b7} step 3 of 9 \u{b7} 1 agent \u{b7} 2 subagents \u{b7} policy: continue 21:30".to_string()), "{t:?}");
        assert!(
            t.contains(&"Draft \u{b7} 5 steps \u{b7} 1 agent \u{b7} not armed".to_string()),
            "{t:?}"
        );
        assert!(
            t.iter()
                .any(|x| x.starts_with("Claude p1 \u{b7} plan/20261112_02")),
            "{t:?}"
        );
        assert!(t.contains(&"Not on a plan".to_string()));
        assert!(t.contains(&"Claude p3".to_string()));
        assert!(!t.iter().any(|x| x.contains("p4")), "no shell");
        // The task's glyph, autocontinue on: a tap turns it off.
        assert!(l.hits().iter().any(|(_, h)| *h
            == ListHit::Autocontinue {
                path: "docs/design/20261112_02_A_PLAN.md".into(),
                on: false
            }));
        // No glyph on a plan that is no task.
        assert!(!l.hits().iter().any(
            |(_, h)| matches!(h, ListHit::Autocontinue { path, .. } if path.contains("_03_"))
        ));
        // The question's mark and its Yes.
        assert!(l.hits().iter().any(|(_, h)| *h
            == ListHit::Answer {
                pane_id: "p2".into(),
                yes: true
            }));
    }

    #[test]
    fn taps_fold_open_and_answer() {
        let mut l = list();
        let agent_rect = l
            .hits()
            .into_iter()
            .find(|(_, h)| *h == ListHit::Agent("p1".into()))
            .unwrap()
            .0;
        let c = agent_rect.center();
        assert_eq!(
            l.touch(1, TouchPhase::Started, c[0], c[1], Duration::ZERO),
            None
        );
        assert_eq!(
            l.touch(1, TouchPhase::Ended, c[0], c[1], Duration::from_millis(50)),
            Some(ListHit::Agent("p1".into()))
        );
        let fold = l
            .hits()
            .into_iter()
            .find(|(_, h)| *h == ListHit::Fold("docs/design/20261112_02_A_PLAN.md".into()))
            .unwrap()
            .0;
        let c = [fold.x + 20.0, fold.y + 10.0];
        l.touch(2, TouchPhase::Started, c[0], c[1], Duration::ZERO);
        l.touch(2, TouchPhase::Ended, c[0], c[1], Duration::from_millis(50));
        assert!(
            !l.hits()
                .iter()
                .any(|(_, h)| *h == ListHit::Agent("p1".into())),
            "folded"
        );
        assert!(
            l.texts().contains(&"Plan policy agent".to_string()),
            "its header stays"
        );
    }

    #[test]
    fn events_in_order_and_kept_open() {
        let mut l = list();
        l.set_now(Duration::from_secs(1));
        l.confirm_opened(
            "p1",
            7,
            "continue",
            "plugin:policy",
            Some("2026-11-12T21:31:00-05:00"),
        );
        l.set_now(Duration::from_secs(2));
        l.decision(serde_json::from_value(json!({"pane_id": "p1", "decision": "continue", "step": 4, "reason": "Done.", "confirm_id": 7, "summary_serial": 9, "at": "2026-11-12T21:32:00-05:00"})).unwrap());
        let t = l.texts();
        let a = t
            .iter()
            .position(|x| x.starts_with("Approval \u{b7} Plan policy: continue"))
            .unwrap();
        let d = t
            .iter()
            .position(|x| x == "Policy: continue, step 4 \u{2014} waiting for you")
            .unwrap();
        assert!(a < d, "oldest first: {t:?}");
        let approve: Vec<_> = l
            .hits()
            .into_iter()
            .filter(|(_, h)| {
                *h == ListHit::Confirm {
                    confirm_id: 7,
                    approve: true,
                }
            })
            .collect();
        assert_eq!(
            approve.len(),
            2,
            "the confirmation and the waiting continue"
        );
        l.confirm_resolved(7, "sent");
        assert!(
            l.texts()
                .contains(&"Policy: continue, step 4 \u{2014} sent".to_string())
        );
        assert!(
            !l.hits()
                .iter()
                .any(|(_, h)| matches!(h, ListHit::Confirm { .. })),
            "closed"
        );
        // A seventh closed event drops the oldest closed; an open one stays.
        l.confirm_opened("p3", 20, "continue", "paired:phone", None);
        for i in 0..6 {
            l.decision(serde_json::from_value(json!({"pane_id": "p3", "decision": "stop", "reason": format!("r{i}"), "summary_serial": 100 + i})).unwrap());
        }
        let t = l.texts();
        assert!(!t.iter().any(|x| x.ends_with("r0")), "{t:?}");
        assert!(t.iter().any(|x| x.ends_with("r5")));
        assert!(
            t.iter()
                .any(|x| x.starts_with("Approval \u{b7} paired:phone")),
            "the open one stays"
        );
        // A tap on Approve gives exactly its id.
        let r = l
            .hits()
            .into_iter()
            .find(|(_, h)| {
                *h == ListHit::Confirm {
                    confirm_id: 20,
                    approve: true,
                }
            })
            .unwrap()
            .0;
        let c = r.center();
        l.touch(3, TouchPhase::Started, c[0], c[1], Duration::ZERO);
        assert_eq!(
            l.touch(3, TouchPhase::Ended, c[0], c[1], Duration::from_millis(40)),
            Some(ListHit::Confirm {
                confirm_id: 20,
                approve: true
            })
        );
    }

    #[test]
    fn without_the_plugin_one_group_and_a_line() {
        let mut l = list();
        l.set_overview(None, Grouping::PluginOff);
        let t = l.texts();
        assert_eq!(t[0], copy::TASKS_TURN_ON.text);
        assert!(t.contains(&"Not on a plan".to_string()));
        assert!(t.contains(&"Claude p1".to_string()), "{t:?}");
        assert!(l.hits().iter().any(|(_, h)| *h == ListHit::Settings));
        l.set_overview(None, Grouping::OlderFlankr);
        assert_eq!(l.texts()[0], copy::TASKS_OLDER_FLANKR.text);
        assert_eq!(clock("2026-11-12T21:30:00-05:00").as_deref(), Some("21:30"));
        assert_eq!(clock("x"), None);
    }
}
