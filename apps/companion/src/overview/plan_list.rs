//! The Active tasks view (docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md,
//! 2.3 to 2.5, with its amendments): the home screen's list of plans, in
//! place of the card grid while *Home view* is *Active tasks*.
//!
//! Grouped by project (the plan's 2026-11-13 01:52 EST amendment):
//!
//! - **A project's line**, its folder's name; projects by most recent
//!   activity, the order kept while a finger was down in the last
//!   [`RESORT_AFTER`]; tap to fold.
//! - **A plan's row** per plan of the project, 2 lines: its title and
//!   waiting marks (amber, confirmations; blue, questions), and, for a
//!   task (a plan Build was chosen on), the right-side glyph that turns
//!   its autocontinue on and off; then, dim, its stage (*Draft*, *step N
//!   of M* while built, *In implementation · idle*, *Implemented*), its
//!   agents as short labels ("claude · w7"), the policy's last word (or
//!   *not armed* in amber for a task), and *N open questions*. A tap
//!   opens the agent view of its agent; a plan with none folds.
//! - **Its subagents** under a plan being built, one line each; no agent
//!   row.
//! - **Its events** under them, oldest first, at most [`EVENTS_MAX`] per
//!   agent: its
//!   open question (Yes / No), its confirmations (Approve / Deny), and the
//!   policy's decisions (a waiting `continue`: Approve / Deny; `ask`:
//!   Continue and Open; `stop`: Open). Closed events older than
//!   [`CLOSED_KEPT`] go.
//! - **Not on a plan** last, with an agent line per pane (its name and
//!   worktree; its turn, running subagents and policy line) and its
//!   events; and with the plugin off or missing, one line at the top and
//!   every agent there.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use flankr_companion::policy::{
    PlanGroup, PolicyAgentLink, PolicyDecision, PolicyOverview, PolicyPlanGroup, ProjectGroup,
    in_project,
};
use libcontrols::{DrawContext, Rect};
use libflankr_proto::common::{AgentInfo, ChildInfo, Turn};
use libmorph::TouchPhase;
use libmsdf::drawlist::DrawList;

use crate::agent::{Row, Terminal, draw_row, fill, line_h, term_h};
use crate::controls::scroll::{Scroll, Touched};
use crate::settings::copy;
use crate::settings::remote_details::draw_chevron;
use crate::theme::theme;

/// How long a row is quiet before it folds under **N inactive** (the
/// active tasks overview plan, 2.3 item 2).
pub const INACTIVE_AFTER: Duration = Duration::from_secs(10 * 60);

/// The most event rows under one agent.
pub const EVENTS_MAX: usize = 6;
/// How long a closed event stays.
pub const CLOSED_KEPT: Duration = Duration::from_secs(30 * 60);

/// How long no finger has to be down before the projects are sorted
/// again by activity (the 21:58 EST amendment, item 3).
pub const RESORT_AFTER: Duration = Duration::from_secs(2);

/// Heights and sizes, in points (2.3 item 5).
const PROJECT_H: f32 = 30.0;
const HEADER_H: f32 = 40.0;
const AGENT_H: f32 = 44.0;
const CHILD_H: f32 = 16.0;
const SUBAGENT_H: f32 = 22.0;
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
    /// A pull to refresh let go past its threshold: `overview` and the
    /// agent list are asked again (the 2026-11-13 01:07 EST amendment).
    Refresh,
    /// *N hidden · Show*: everything swiped away shows again.
    ShowHidden,
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
    /// Its identity, for a swipe: a confirmation's id, a decision's pane
    /// and serial.
    fn key(&self, pane: &str) -> String {
        match &self.kind {
            EventKind::Confirmation { confirm_id, .. } => format!("confirm/{confirm_id}"),
            EventKind::Decision(d) => format!("decision/{pane}/{}", d.summary_serial),
        }
    }

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
#[derive(Clone, Debug, Default, PartialEq)]
struct Item {
    rect: Rect,
    texts: Vec<(String, [f32; 2], f32, [f32; 4])>,
    fills: Vec<(Rect, [f32; 4], f32)>,
    hits: Vec<(Rect, ListHit)>,
    /// What a sideways swipe on it hides, if anything.
    key: Option<Hide>,
    /// Its glyphs, from the terminal face's symbol page (2.1).
    glyphs: Vec<Glyph>,
    /// Its drawn chevrons (2.4): where each points from, open (down) or
    /// closed (right), and its ink.
    chevrons: Vec<([f32; 2], bool, [f32; 4])>,
}

impl Item {
    /// Moves it down by `dy`.
    fn shift(&mut self, dy: f32) {
        self.rect.y += dy;
        for t in &mut self.texts {
            t.1[1] += dy;
        }
        for f in &mut self.fills {
            f.0.y += dy;
        }
        for h in &mut self.hits {
            h.0.y += dy;
        }
        for g in &mut self.glyphs {
            g.at[1] += dy;
        }
        for c in &mut self.chevrons {
            c.0[1] += dy;
        }
    }
}

/// A glyph of a row (the active tasks overview plan, 2.1): a kind's, at
/// the left of the row, or a mark after one of its texts.
#[derive(Clone, Debug, PartialEq)]
struct Glyph {
    text: String,
    at: [f32; 2],
    px: f32,
    colour: [f32; 4],
    /// Drawn after the row's text of this index, where the face puts its
    /// end, rather than at `at[0]`.
    after: Option<usize>,
    /// The row is working: the drawer's Braille spinner in place of the
    /// glyph (2.2 item 5).
    spin: bool,
}

/// The kinds of row (the active tasks overview plan, 2.1), each drawn
/// with its glyph from the terminal face's symbol page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A plan from the planning plugin: ☑.
    Plan,
    /// One of a pane's subagents: ⑂.
    Subagent,
    /// The policy's autocontinue on a plan's task: ↻.
    Autocontinue,
    /// An agent's own goal: ◎.
    Goal,
}

impl Kind {
    /// Its glyph.
    pub fn glyph(self) -> &'static str {
        match self {
            Kind::Plan => crate::titlebar::PLANS_GLYPH,
            Kind::Subagent => "\u{2442}",
            Kind::Autocontinue => "\u{21bb}",
            Kind::Goal => "\u{25ce}",
        }
    }
}

/// The column a kind's glyph takes at the left of a row, in points.
const KIND_W: f32 = 20.0;
/// The gap before a mark after a text, in points.
const MARK_GAP: f32 = 6.0;
/// The project title's arrow's touch area: the right of the row, in
/// points (2.5 item 2).
const ARROW_W: f32 = 48.0;
/// The chevron's column at the left of a collapsible row, in points.
const CHEVRON_W: f32 = 18.0;

/// What a swipe hides (the 2026-11-13 01:07 EST amendment, items 2 and 3).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Hide {
    /// An event row, by its identity.
    Event(String),
    /// A plan's group, by its path.
    Task(String),
}

/// A touch held on the list: where it came down, and whether the list
/// was at its top then (a pull to refresh can start).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gesture {
    id: u64,
    x0: f32,
    y0: f32,
    at_top: bool,
    pulling: bool,
}

/// Pull to refresh, as the agent view's: the movement counts at this
/// share, up to `PULL_MAX` points; let go past `PULL_ARM`, it refreshes.
const PULL_RESIST: f32 = 0.5;
const PULL_MAX: f32 = 96.0;
const PULL_ARM: f32 = 64.0;
/// The longest a refresh's line shows without an answer.
pub const REFRESH_MAX: Duration = Duration::from_secs(5);
/// How far a finger goes sideways for a swipe, in points.
const SWIPE: f32 = 72.0;

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
    /// Event rows swiped away, by identity.
    hidden_events: HashSet<String>,
    /// Tasks swiped away: their state then and their event rows then.
    hidden_tasks: HashMap<String, (String, HashSet<String>)>,
    /// How many are hidden in the layout.
    hidden: usize,
    gesture: Option<Gesture>,
    /// How far the list is pulled down, in pixels.
    pull: f32,
    /// A refresh asked, and how long ago.
    refreshing: Option<Duration>,
    /// The projects' order as last sorted, by root, and how long since a
    /// finger was down.
    order: Vec<String>,
    since_touch: Duration,
    /// The wall clock in tests; the system's otherwise.
    wall: Option<f64>,
    /// The spinner's clock, and the frame last drawn while one spins.
    clock: Duration,
    frame: Option<usize>,
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
            hidden_events: HashSet::new(),
            hidden_tasks: HashMap::new(),
            hidden: 0,
            gesture: None,
            pull: 0.0,
            refreshing: None,
            order: Vec::new(),
            since_touch: RESORT_AFTER,
            wall: None,
            clock: Duration::ZERO,
            frame: None,
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

    /// The spinner's clock, the drawer's (the app's time since it
    /// started), before each draw (2.2 item 5): the list is drawn again
    /// only when a spinning row is in view and the frame moved on.
    pub fn set_clock(&mut self, clock: Duration) {
        self.clock = clock;
        let frame = self
            .spinning()
            .then(|| crate::drawer::spinner::frame_at(clock));
        if frame != self.frame {
            self.frame = frame;
            self.dirty = true;
        }
    }

    /// Whether a working row's spinner is in view.
    pub fn spinning(&self) -> bool {
        let top = self.scroll.offset();
        let (vy, vh) = (top, self.viewport.h);
        self.items.iter().any(|i| {
            i.glyphs.iter().any(|g| g.spin) && i.rect.y + i.rect.h >= vy && i.rect.y <= vy + vh
        })
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
        self.projects(&self.order)
            .into_iter()
            .flat_map(|p| p.plans)
            .collect()
    }

    /// The groups by project, the projects in `order` first.
    fn projects(&self, order: &[String]) -> Vec<ProjectGroup> {
        match (&self.overview, self.grouping) {
            (Some(o), Grouping::ByPlan) => o.by_project(&self.agents, order),
            _ => PolicyOverview::default().by_project(&self.agents, order),
        }
    }

    fn relayout(&mut self) {
        self.dirty = true;
        let s = self.s;
        let w = self.viewport.w;
        let mut y = 0.0;
        let mut items = Vec::new();
        let mut hidden = 0;
        // The pull's line, while pulled or refreshing.
        let pull_h = if self.refreshing.is_some() {
            PULL_ARM * s
        } else {
            self.pull
        };
        if pull_h > 0.0 {
            let text = if self.refreshing.is_some() {
                crate::agent::REFRESHING
            } else if self.pull >= PULL_ARM * s {
                crate::agent::RELEASE_TO_REFRESH
            } else {
                crate::agent::PULL_TO_REFRESH
            };
            items.push(Item {
                rect: Rect::new(0.0, y, w, pull_h),
                texts: vec![(
                    text.to_string(),
                    [PAD * s, y + (pull_h - line_h(DIM_PT * s)) * 0.5],
                    DIM_PT * s,
                    theme().dim,
                )],
                fills: Vec::new(),
                hits: Vec::new(),
                key: None,
                ..Default::default()
            });
            y += pull_h;
        }
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
                key: None,
                ..Default::default()
            });
            y += h;
        }
        // Sorted again by activity once no finger has been down for
        // RESORT_AFTER; else in the order kept.
        if self.since_touch >= RESORT_AFTER || self.order.is_empty() {
            self.order = self
                .projects(&[])
                .iter()
                .map(|p| p.key().to_string())
                .collect();
        }
        let projects = self.projects(&self.order);
        let linked = projects
            .iter()
            .flat_map(|p| p.plans.iter())
            .filter(|g| g.plan.is_some())
            .count();
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
                key: None,
                ..Default::default()
            });
            y += h;
        }
        // A task swiped away stays hidden until a new event row or a
        // state change (the 2026-11-13 01:07 EST amendment, item 3).
        let mut back: Vec<String> = Vec::new();
        // Each goal row goes under the project its agent's folder lies in,
        // else under its agent in Not on a plan (2.1 item 2).
        let goals = self.goal_agents(&projects);
        let goal_home = |a: &AgentInfo| {
            projects
                .iter()
                .filter_map(|p| p.project.as_ref())
                .find(|p| !p.root.is_empty() && in_project(a, &p.root))
                .map(|p| p.root.clone())
        };
        let homed: HashSet<String> = goals
            .iter()
            .filter(|(a, _)| goal_home(a).is_some())
            .map(|(a, _)| a.pane_id.clone())
            .collect();
        for project in &projects {
            let key = format!("project:{}", project.key());
            if let Some(pr) = &project.project {
                // Its arrow opens its main agent (2.5).
                let main = project.main_agent(&self.agents).map(|a| a.pane_id.clone());
                let item = self.project_line(pr, &key, main, y, w);
                y += item.rect.h;
                items.push(item);
                if self.folded.contains(&key) {
                    continue;
                }
            }
            // Each row and what goes under it, laid out from 0, and
            // whether it is inactive (the active tasks overview plan, 2.3).
            let mut units: Vec<(bool, Vec<Item>)> = Vec::new();
            let mut unit = |inactive: bool, list: Vec<Item>| {
                if !list.is_empty() {
                    units.push((inactive, list));
                }
            };
            for g in &project.plans {
                if g.plan.is_none() && g.agents.is_empty() {
                    continue;
                }
                if let Some(p) = &g.plan
                    && let Some((sig, keys)) = self.hidden_tasks.get(&p.path)
                {
                    let now_keys = self.event_keys(g);
                    if *sig == task_state(p, g) && now_keys.is_subset(keys) {
                        hidden += 1;
                        continue;
                    }
                    back.push(p.path.clone());
                }
                let mut list = Vec::new();
                let mut uy = 0.0;
                let header = self.header(g, uy, w);
                uy += header.rect.h;
                list.push(header);
                let fold = g.plan.as_ref().map_or(String::new(), |p| p.path.clone());
                if g.plan.is_some() {
                    let active = self.plan_active(g);
                    if !self.folded.contains(&fold) {
                        // A plan's subagents, then its agents' events: no
                        // agent row (the 01:52 EST amendment, items 3 and
                        // 5); an active plan's old subagents fold away.
                        let item = self
                            .subagent_lines(g, uy, w, |c| !active || child_active(c, self.wall()));
                        if item.rect.h > 0.0 {
                            uy += item.rect.h;
                            list.push(item);
                        }
                        for (agent, _) in &g.agents {
                            for item in self.event_rows(agent, uy, w) {
                                if let Some(Hide::Event(k)) = &item.key
                                    && self.hidden_events.contains(k)
                                {
                                    hidden += 1;
                                    continue;
                                }
                                uy += item.rect.h;
                                list.push(item);
                            }
                        }
                    }
                    unit(!active, list);
                    if active && !self.folded.contains(&fold) {
                        let old = self.subagent_lines(g, 0.0, w, |c| !child_active(c, self.wall()));
                        if old.rect.h > 0.0 {
                            unit(true, vec![old]);
                        }
                    }
                    continue;
                }
                // Not on a plan: its line, then each agent as a row.
                unit(false, list);
                if self.folded.contains(&fold) {
                    continue;
                }
                for (agent, link) in &g.agents {
                    let mut list = Vec::new();
                    let mut uy = 0.0;
                    let item = self.agent_line(agent, link.as_ref(), uy, w);
                    uy += item.rect.h;
                    list.push(item);
                    // Its goal under it, when no project took it.
                    if let Some((a, goal)) = goals.iter().find(|(a, _)| a.pane_id == agent.pane_id)
                        && !homed.contains(&a.pane_id)
                    {
                        let item = self.goal_line(a, goal, uy, w);
                        uy += item.rect.h;
                        list.push(item);
                    }
                    for item in self.event_rows(agent, uy, w) {
                        if let Some(Hide::Event(k)) = &item.key
                            && self.hidden_events.contains(k)
                        {
                            hidden += 1;
                            continue;
                        }
                        list.push(item);
                    }
                    unit(!self.agent_active(agent, link.as_ref()), list);
                }
            }
            // The project's goal rows, after its plans.
            if let Some(pr) = &project.project {
                for (a, goal) in &goals {
                    if goal_home(a).as_deref() == Some(pr.root.as_str()) {
                        let item = self.goal_line(a, goal, 0.0, w);
                        unit(!self.agent_active(a, None), vec![item]);
                    }
                }
            }
            // The active rows, then the closed **N inactive** section.
            let inactive = units.iter().filter(|(i, _)| *i).count();
            for (_, list) in units.iter_mut().filter(|(i, _)| !*i) {
                for mut item in list.drain(..) {
                    item.shift(y);
                    y += item.rect.h;
                    items.push(item);
                }
            }
            if inactive > 0 {
                let ikey = format!("inactive:{}", project.key());
                // Kept in `folded` while open: closed by default (2.3 item 4).
                let open = self.folded.contains(&ikey);
                let item = self.inactive_line(inactive, &ikey, open, y, w);
                y += item.rect.h;
                items.push(item);
                if open {
                    for (_, list) in units.iter_mut().filter(|(i, _)| *i) {
                        for mut item in list.drain(..) {
                            item.shift(y);
                            y += item.rect.h;
                            items.push(item);
                        }
                    }
                }
            }
        }
        if hidden > 0 {
            let h = 28.0 * s;
            items.push(Item {
                rect: Rect::new(0.0, y, w, h),
                texts: vec![(
                    format!("{hidden} hidden \u{b7} {}", copy::TASKS_SHOW.text),
                    [PAD * s, y + 6.0 * s],
                    DIM_PT * s,
                    theme().dim,
                )],
                fills: Vec::new(),
                hits: vec![(Rect::new(0.0, y, w, h), ListHit::ShowHidden)],
                key: None,
                ..Default::default()
            });
            y += h;
        }
        for path in back {
            self.hidden_tasks.remove(&path);
        }
        self.hidden = hidden;
        self.items = items;
        self.height = y;
        self.scroll.set_extent(y, self.viewport.h);
    }

    /// The identities of a group's event rows now.
    fn event_keys(&self, g: &PlanGroup) -> HashSet<String> {
        let mut keys = HashSet::new();
        for (a, _) in &g.agents {
            if let Some(list) = self.events.get(&a.pane_id) {
                keys.extend(list.iter().map(|e| e.key(&a.pane_id)));
            }
            if let Some(q) = question_of(a) {
                keys.insert(question_key(&a.pane_id, &q));
            }
        }
        keys
    }

    /// How many rows and tasks are hidden.
    pub fn hidden(&self) -> usize {
        self.hidden
    }

    /// The refresh's answer came: its line goes.
    pub fn refreshed(&mut self) {
        if self.refreshing.take().is_some() {
            self.relayout();
        }
    }

    /// Hides what `key` names.
    fn hide(&mut self, key: Hide) {
        match key {
            Hide::Event(k) => {
                self.hidden_events.insert(k);
            }
            Hide::Task(path) => {
                let groups = self.groups();
                if let Some(g) = groups
                    .iter()
                    .find(|g| g.plan.as_ref().is_some_and(|p| p.path == path))
                {
                    let sig = task_state(g.plan.as_ref().unwrap(), g);
                    self.hidden_tasks.insert(path, (sig, self.event_keys(g)));
                }
            }
        }
        self.relayout();
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
        // A plan's row starts with ☑, and its title ends in ↻ while its
        // task's autocontinue is on and ◎ while an agent's goal names it
        // (the active tasks overview plan, 2.1 item 1).
        let marks = g
            .plan
            .as_ref()
            .map(|p| plan_marks(p, g))
            .unwrap_or_default();
        let lead = if g.plan.is_some() { KIND_W * s } else { 0.0 };
        let title_w = w
            - 2.0 * PAD * s
            - lead
            - marks.len() as f32 * (TITLE_PT + MARK_GAP) * s
            - glyph.as_ref().map_or(0.0, |(r, _, _)| r.w + 8.0 * s)
            - 48.0 * s;
        let title = match &g.plan {
            Some(p) => p
                .title
                .clone()
                .unwrap_or_else(|| p.path.rsplit('/').next().unwrap_or("").to_string()),
            None => copy::TASKS_NOT_ON_A_PLAN.text.to_string(),
        };
        let mut glyphs = Vec::new();
        if g.plan.is_some() {
            glyphs.push(Glyph {
                text: Kind::Plan.glyph().to_string(),
                at: [PAD * s, y + 4.0 * s],
                px: TITLE_PT * s,
                colour: theme().fg,
                after: None,
                // An agent on it still producing (2.2 item 5).
                spin: g.agents.iter().any(|(a, _)| agent_works(a)),
            });
        }
        texts.push((
            ellipsis(&title, title_w, TITLE_PT * s),
            [PAD * s + lead, y + 4.0 * s],
            TITLE_PT * s,
            theme().fg,
        ));
        let mut after = String::new();
        for k in marks {
            glyphs.push(Glyph {
                text: format!("{after}{}", k.glyph()),
                at: [PAD * s + lead, y + 4.0 * s],
                px: TITLE_PT * s,
                colour: theme().fg,
                after: Some(0),
                spin: false,
            });
            // A second mark after the first, a cell on.
            after.push(' ');
            after.push(' ');
        }
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
                [PAD * s + lead, y + 22.0 * s],
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
        // A plan's row opens its agent's view (the 01:52 EST amendment,
        // item 3); one with no agent, and Not on a plan, fold.
        let open = g
            .plan
            .as_ref()
            .and(g.agents.first())
            .map(|(a, _)| ListHit::Agent(a.pane_id.clone()));
        hits.push((rect, open.unwrap_or(ListHit::Fold(key))));
        let hide = g.plan.as_ref().map(|p| Hide::Task(p.path.clone()));
        Item {
            rect,
            texts,
            fills,
            hits,
            key: hide,
            glyphs,
            ..Default::default()
        }
    }

    /// A project's line: its folder's name; a tap folds it.
    fn project_line(
        &self,
        p: &flankr_companion::policy::PolicyProject,
        key: &str,
        main: Option<String>,
        y: f32,
        w: f32,
    ) -> Item {
        let s = self.s;
        let h = PROJECT_H * s;
        let rect = Rect::new(0.0, y, w, h);
        let name = if p.name.is_empty() {
            copy::TASKS_THIS_PROJECT.text.to_string()
        } else {
            p.name.clone()
        };
        // The drawn chevron, right while folded and down while open (2.4).
        let open = !self.folded.contains(key);
        let mut chevrons = vec![([PAD * s + 6.0 * s, y + h * 0.5], open, theme().accent)];
        let mut hits = vec![(rect, ListHit::Fold(key.to_string()))];
        // The arrow at the right edge, the right-pointing chevron in the
        // secondary ink, with the right ARROW_W of the row its own: it
        // opens the project's main agent (2.5); none without an agent.
        let arrow_w = if main.is_some() { ARROW_W * s } else { 0.0 };
        if let Some(pane) = main {
            chevrons.push(([w - PAD * s - 6.0 * s, y + h * 0.5], false, theme().dim));
            hits.push((
                Rect::new(w - ARROW_W * s, y, ARROW_W * s, h),
                ListHit::Agent(pane),
            ));
        }
        Item {
            rect,
            texts: vec![(
                ellipsis(
                    &name,
                    w - 2.0 * PAD * s - CHEVRON_W * s - arrow_w,
                    LINE_PT * s,
                ),
                [(PAD + CHEVRON_W) * s, y + (h - line_h(LINE_PT * s)) * 0.5],
                LINE_PT * s,
                theme().accent,
            )],
            fills: vec![(Rect::new(0.0, y, w, h), theme().card, 0.0)],
            hits,
            key: None,
            chevrons,
            ..Default::default()
        }
    }

    /// A plan's running subagents, one line each, under its row.
    fn subagent_lines(
        &self,
        g: &PlanGroup,
        y: f32,
        w: f32,
        keep: impl Fn(&ChildInfo) -> bool,
    ) -> Item {
        let s = self.s;
        let x = (PAD + INDENT) * s;
        let mut texts = Vec::new();
        let mut glyphs = Vec::new();
        let mut at = y;
        for (a, _) in &g.agents {
            for c in a.children.iter().filter(|c| shown_child(c) && keep(c)) {
                // ⑂, then its words (2.1 item 3); the spinner while it
                // works (2.2 items 3 and 5).
                glyphs.push(Glyph {
                    text: Kind::Subagent.glyph().to_string(),
                    at: [x, at + 2.0 * s],
                    px: LINE_PT * s,
                    colour: theme().fg,
                    after: None,
                    spin: child_works(c),
                });
                texts.push((
                    ellipsis(&c.short, w - x - KIND_W * s - PAD * s, LINE_PT * s),
                    [x + KIND_W * s, at + 2.0 * s],
                    LINE_PT * s,
                    theme().fg,
                ));
                at += SUBAGENT_H * s;
            }
        }
        Item {
            rect: Rect::new(0.0, y, w, at - y),
            texts,
            fills: Vec::new(),
            hits: Vec::new(),
            key: None,
            glyphs,
            ..Default::default()
        }
    }

    /// An agent's own goal that names no plan (2.1 item 2): ◎ and the
    /// goal cut to one line, the spinner while its agent works; a tap
    /// opens the agent.
    fn goal_line(&self, a: &AgentInfo, goal: &str, y: f32, w: f32) -> Item {
        let s = self.s;
        let h = SUBAGENT_H * s + 4.0 * s;
        let rect = Rect::new(0.0, y, w, h);
        let x = PAD * s;
        Item {
            rect,
            texts: vec![(
                ellipsis(goal, w - x - KIND_W * s - PAD * s, LINE_PT * s),
                [x + KIND_W * s, y + 4.0 * s],
                LINE_PT * s,
                theme().fg,
            )],
            hits: vec![(rect, ListHit::Agent(a.pane_id.clone()))],
            glyphs: vec![Glyph {
                text: Kind::Goal.glyph().to_string(),
                at: [x, y + 4.0 * s],
                px: LINE_PT * s,
                colour: theme().fg,
                after: None,
                spin: agent_works(a),
            }],
            ..Default::default()
        }
    }

    /// The agents whose goal names no plan here: none of the plans has
    /// them linked by their goal.
    fn goal_agents(&self, projects: &[ProjectGroup]) -> Vec<(AgentInfo, String)> {
        let by_goal: HashSet<&str> = projects
            .iter()
            .flat_map(|p| p.plans.iter())
            .filter(|g| g.plan.is_some())
            .flat_map(|g| g.agents.iter())
            .filter(|(_, l)| l.as_ref().is_some_and(|l| l.linked_by == "goal"))
            .map(|(a, _)| a.pane_id.as_str())
            .collect();
        self.agents
            .iter()
            .filter(|a| a.agent != "shell" && !by_goal.contains(a.pane_id.as_str()))
            .filter_map(|a| {
                let goal = a.goal.as_deref()?.trim();
                (!goal.is_empty())
                    .then(|| (a.clone(), goal.lines().next().unwrap_or("").to_string()))
            })
            .collect()
    }

    /// The **N inactive** row (2.3): its chevron, right while closed;
    /// a tap opens and closes it.
    fn inactive_line(&self, n: usize, key: &str, open: bool, y: f32, w: f32) -> Item {
        let s = self.s;
        let h = 28.0 * s;
        let rect = Rect::new(0.0, y, w, h);
        Item {
            rect,
            texts: vec![(
                copy::TASKS_INACTIVE
                    .fill(None, &[("n", &n.to_string())])
                    .trim()
                    .to_string(),
                [(PAD + CHEVRON_W) * s, y + (h - line_h(DIM_PT * s)) * 0.5],
                DIM_PT * s,
                theme().dim,
            )],
            hits: vec![(rect, ListHit::Fold(key.to_string()))],
            chevrons: vec![([PAD * s + 6.0 * s, y + h * 0.5], open, theme().dim)],
            ..Default::default()
        }
    }

    /// The wall clock, in seconds since 1970, for `last_activity`'s age.
    fn wall(&self) -> f64 {
        self.wall.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64())
        })
    }

    /// Sets the wall clock the ages are taken at (tests).
    #[cfg(test)]
    pub fn set_wall(&mut self, secs: f64) {
        self.wall = Some(secs);
        self.relayout();
    }

    /// Whether something of `a` waits on the owner (2.3 item 3): its
    /// question, a confirmation open on its pane, or the policy's `ask`.
    fn waits(&self, a: &AgentInfo, link: Option<&PolicyAgentLink>) -> bool {
        question_of(a).is_some()
            || self.confirm_panes.iter().any(|(id, p)| {
                *p == a.pane_id && self.confirms.get(id).is_some_and(Option::is_none)
            })
            || link
                .and_then(|l| l.last_decision.as_ref())
                .is_some_and(|d| d.decision == "ask")
            || self.events.get(&a.pane_id).is_some_and(|l| {
                l.iter()
                    .rev()
                    .find_map(|e| match &e.kind {
                        EventKind::Decision(d) => Some(d.decision == "ask"),
                        _ => None,
                    })
                    .unwrap_or(false)
            })
    }

    /// Whether `a`'s row is active (2.3 items 2 and 3): working, active
    /// within [`INACTIVE_AFTER`], or waiting on the owner.
    fn agent_active(&self, a: &AgentInfo, link: Option<&PolicyAgentLink>) -> bool {
        agent_works(a)
            || age_secs(a, self.wall()).is_some_and(|t| t <= INACTIVE_AFTER.as_secs_f64())
            || self.waits(a, link)
    }

    /// Whether a plan's row is active: one of its agents' is.
    fn plan_active(&self, g: &PlanGroup) -> bool {
        g.agents
            .iter()
            .any(|(a, l)| self.agent_active(a, l.as_ref()))
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
        // An agent with no kind of its own spins in its indent while it
        // works (2.2 item 5).
        let glyphs = if agent_works(a) {
            vec![Glyph {
                text: String::new(),
                at: [PAD * s + 5.0 * s, y + 4.0 * s],
                px: LINE_PT * s,
                colour: theme().fg,
                after: None,
                spin: true,
            }]
        } else {
            Vec::new()
        };
        Item {
            rect,
            texts,
            fills,
            hits: vec![(rect, ListHit::Agent(a.pane_id.clone()))],
            key: None,
            glyphs,
            ..Default::default()
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
            rows.push((
                e.time.clone(),
                e.daemon_time,
                label,
                colour,
                buttons,
                e.key(&a.pane_id),
            ));
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
                question_key(&a.pane_id, &q),
            ));
        }
        let mut out = Vec::new();
        for (time, daemon_time, text, colour, buttons, key) in rows {
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
                key: Some(Hide::Event(key)),
                ..Default::default()
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

    /// A touch: a drag scrolls; a pull down from the top past
    /// `PULL_ARM` refreshes; a sideways swipe hides the row or task under
    /// it; a tap gives its hit (and folds a group, or shows the hidden,
    /// itself).
    pub fn touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        x: f32,
        y: f32,
        now: Duration,
    ) -> Option<ListHit> {
        let s = self.s;
        self.since_touch = Duration::ZERO;
        if phase == TouchPhase::Started {
            self.gesture = Some(Gesture {
                id,
                x0: x,
                y0: y,
                at_top: self.scroll.offset() <= 0.5,
                pulling: false,
            });
        }
        let scrolled = self.scroll.touch(id, phase, x, y, now, s);
        let Some(mut g) = self.gesture.filter(|g| g.id == id) else {
            return self.scrolled(scrolled, x, y);
        };
        let (dx, dy) = (x - g.x0, y - g.y0);
        match phase {
            TouchPhase::Moved => {
                if g.at_top && self.refreshing.is_none() && dy > 0.0 && dy > dx.abs() {
                    g.pulling = true;
                    self.pull = (dy * PULL_RESIST).min(PULL_MAX * s);
                    self.relayout();
                } else if g.pulling {
                    self.pull = 0.0;
                    self.relayout();
                }
                self.gesture = Some(g);
                None
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.gesture = None;
                let pulled = std::mem::take(&mut self.pull);
                if g.pulling {
                    let armed = phase == TouchPhase::Ended && pulled >= PULL_ARM * s;
                    if armed {
                        self.refreshing = Some(Duration::ZERO);
                    }
                    self.relayout();
                    return armed.then_some(ListHit::Refresh);
                }
                if phase == TouchPhase::Ended && dx.abs() >= SWIPE * s && dx.abs() > 2.0 * dy.abs()
                {
                    if let Some(key) = self.key_at(g.x0, g.y0) {
                        self.hide(key);
                    }
                    return None;
                }
                self.scrolled(scrolled, x, y)
            }
            _ => self.scrolled(scrolled, x, y),
        }
    }

    fn scrolled(&mut self, touched: Touched, x: f32, y: f32) -> Option<ListHit> {
        match touched {
            Touched::Scrolled => {
                self.dirty = true;
                None
            }
            Touched::Tap => {
                let hit = self.hit_at(x, y)?;
                match &hit {
                    ListHit::Fold(key) => {
                        if !self.folded.remove(key) {
                            self.folded.insert(key.clone());
                        }
                        self.relayout();
                    }
                    ListHit::ShowHidden => {
                        self.hidden_events.clear();
                        self.hidden_tasks.clear();
                        self.relayout();
                    }
                    _ => {}
                }
                Some(hit)
            }
            Touched::Nothing => None,
        }
    }

    /// What a swipe starting at `(x, y)` on the screen hides.
    fn key_at(&self, x: f32, y: f32) -> Option<Hide> {
        let cy = y - self.viewport.y + self.scroll.offset();
        self.items
            .iter()
            .find(|i| i.rect.contains([x, cy]))
            .and_then(|i| i.key.clone())
    }

    pub fn tick(&mut self, dt: Duration) -> bool {
        // The projects sorted again once no finger has been down for a
        // while, if their order moved.
        let was = self.since_touch;
        self.since_touch += dt;
        let mut cleared = false;
        if was < RESORT_AFTER && self.since_touch >= RESORT_AFTER {
            let order: Vec<String> = self
                .projects(&[])
                .iter()
                .map(|p| p.key().to_string())
                .collect();
            if order != self.order {
                self.relayout();
                cleared = true;
            }
        }
        // A refresh's line goes after `REFRESH_MAX` without an answer.
        if let Some(t) = self.refreshing.as_mut() {
            *t += dt;
            if *t >= REFRESH_MAX {
                self.refreshing = None;
                self.relayout();
                cleared = true;
            }
        }
        let moved = self.scroll.tick(dt, self.s) || cleared;
        if moved {
            self.dirty = true;
        }
        moved
    }

    /// Draws the list; its glyphs in the terminal face when it is given
    /// (2.1), else in the context's font.
    pub fn draw(&self, list: &mut DrawList, ctx: &DrawContext, term: Option<&Terminal>) {
        let s = self.s;
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
            for g in &item.glyphs {
                let ty = g.at[1] + top;
                if ty < vy - line_h(g.px) * 0.5 || ty > vy + vh {
                    continue;
                }
                let x = match g.after.and_then(|i| item.texts.get(i)) {
                    Some((text, at, px, _)) => at[0] + ctx.text_advance(text, *px) + MARK_GAP * s,
                    None => g.at[0],
                };
                let text = if g.spin {
                    crate::drawer::spinner::glyph_at(self.clock).to_string()
                } else {
                    g.text.clone()
                };
                let row = Row {
                    text,
                    x: 0.0,
                    y: 0.0,
                    px: g.px,
                    color: g.colour,
                    mono: true,
                };
                // The face's line sits as the text's does.
                let dy = (line_h(g.px) - term_h(g.px)) * 0.5;
                let dy = if term.is_some() { dy } else { 0.0 };
                draw_row(list, ctx, term, &row, [x, ty + dy]);
            }
            for (at, open, colour) in &item.chevrons {
                let c = [at[0], at[1] + top];
                if c[1] < vy || c[1] > vy + vh {
                    continue;
                }
                draw_chevron(list, c, *open, s, *colour);
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

    /// The glyphs laid out, top to bottom (tests).
    pub fn glyphs(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| {
                i.glyphs.iter().map(|g| {
                    if g.spin {
                        crate::drawer::spinner::glyph_at(self.clock).to_string()
                    } else {
                        g.text.trim().to_string()
                    }
                })
            })
            .collect()
    }

    /// The chevrons laid out, top to bottom: open (down) or not (tests).
    pub fn chevrons(&self) -> Vec<bool> {
        self.items
            .iter()
            .flat_map(|i| i.chevrons.iter().map(|c| c.1))
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
/// And the identity a swipe hides it by.
type RowSpec = (
    Option<String>,
    bool,
    String,
    [f32; 4],
    Vec<(String, ListHit)>,
    String,
);

/// A question row's identity: its pane and its text.
fn question_key(pane: &str, question: &str) -> String {
    format!("question/{pane}/{question}")
}

/// A task's state, for its swipe (the 2026-11-13 01:07 EST amendment,
/// item 3): armed, autocontinue, status and current step; whether any of
/// its agents asks is in its event rows.
fn task_state(p: &PolicyPlanGroup, _g: &PlanGroup) -> String {
    format!(
        "{}|{}|{}|{:?}",
        p.armed,
        p.autocontinue,
        p.status.as_deref().unwrap_or(""),
        p.current_step
    )
}

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

/// A plan row's dim line (the 2026-11-13 01:52 EST amendment, items 2,
/// 3 and 6): its stage (*step N of M* while an agent builds it, *In
/// implementation · idle* with none, else its status), its agents as
/// short labels, the policy's last word (or *not armed* for a task not
/// armed), and *N open questions*.
pub fn header_line(p: &PolicyPlanGroup, g: &PlanGroup) -> String {
    let mut parts: Vec<String> = Vec::new();
    let status = p.status.clone().unwrap_or_default();
    let building = !g.agents.is_empty();
    let implementing = status.starts_with("In implementation");
    match (building, implementing, p.current_step, p.steps_total) {
        (true, true, Some(n), Some(t)) => parts.push(format!("step {n} of {t}")),
        (false, true, _, _) => {
            parts.push(status.clone());
            parts.push(copy::TASKS_IDLE.text.to_string());
        }
        _ if !status.is_empty() => parts.push(status.clone()),
        _ => {}
    }
    for (a, link) in &g.agents {
        parts.push(agent_label(a, link.as_ref()));
    }
    if p.task && !p.armed {
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
    match p.open_questions.unwrap_or(0) {
        0 => {}
        1 => parts.push(copy::TASKS_OPEN_QUESTION.text.to_string()),
        n => parts.push(
            copy::TASKS_OPEN_QUESTIONS
                .fill(None, &[("n", &n.to_string())])
                .trim()
                .to_string(),
        ),
    }
    parts.join(" \u{b7} ")
}

/// An agent as a plan row names it: its kind and its worktree's label
/// or branch, "claude · w7".
fn agent_label(a: &AgentInfo, link: Option<&PolicyAgentLink>) -> String {
    let tree = link
        .and_then(|l| l.worktree.as_ref())
        .and_then(|wt| wt.label.clone().or(wt.branch.clone()))
        .or_else(|| {
            a.worktree
                .as_ref()
                .and_then(|wt| wt.label.clone().or(wt.branch.clone()))
        });
    let kind = a.agent.to_lowercase();
    match tree {
        Some(t) => format!("{kind} \u{b7} {t}"),
        None => kind,
    }
}

/// Whether `a` is still producing (2.2): the daemon's `working`; from an
/// older daemon, which sends no `last_activity`, herdr's `working`.
pub fn agent_works(a: &AgentInfo) -> bool {
    a.working || (a.last_activity.is_none() && a.status == "working")
}

/// Seconds since `a` last produced, at `wall`: from its `last_activity`,
/// else (an older daemon) its `idle_ms`; `None` when neither is known.
fn age_secs(a: &AgentInfo, wall: f64) -> Option<f64> {
    flankr_companion::policy::last_activity_secs(a)
        .map(|t| (wall - t).max(0.0))
        .or_else(|| a.idle_ms.map(|ms| ms as f64 / 1000.0))
}

/// Whether subagent `c` is listed at all: running, or, from a daemon that
/// says when it last produced, finished (an older one's finished stay
/// out, as before).
fn shown_child(c: &ChildInfo) -> bool {
    !c.finished || child_works(c) || c.last_activity.is_some()
}

/// Whether subagent `c`'s line is active at `wall` (2.3 item 2): working,
/// or written within [`INACTIVE_AFTER`]; from an older daemon, not
/// finished.
fn child_active(c: &ChildInfo, wall: f64) -> bool {
    child_works(c)
        || match c
            .last_activity
            .as_deref()
            .and_then(flankr_companion::cross::rfc3339_secs)
        {
            Some(t) => wall - t <= INACTIVE_AFTER.as_secs_f64(),
            None => !c.finished,
        }
}

/// Whether subagent `c` is still producing (2.2 item 3): the daemon's
/// `working`; from an older daemon, not finished.
pub fn child_works(c: &ChildInfo) -> bool {
    c.working || (c.last_activity.is_none() && !c.finished)
}

/// The marks after a plan's title (2.1 item 1): ↻ while its task's
/// autocontinue is on, ◎ while an agent's goal names it.
fn plan_marks(p: &PolicyPlanGroup, g: &PlanGroup) -> Vec<Kind> {
    let mut marks = Vec::new();
    if p.task && p.autocontinue {
        marks.push(Kind::Autocontinue);
    }
    if g.agents
        .iter()
        .any(|(_, l)| l.as_ref().is_some_and(|l| l.linked_by == "goal"))
    {
        marks.push(Kind::Goal);
    }
    marks
}

fn header_colour(p: &PolicyPlanGroup) -> [f32; 4] {
    if p.task && !p.armed {
        theme().accent
    } else {
        theme().dim
    }
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
                 "project": {"root": "/r/flankr", "name": "flankr"},
                 "status": "In implementation", "armed": true, "task": true, "autocontinue": true,
                 "steps_total": 9, "current_step": 3,
                 "agents": [{"pane_id": "p1", "linked_by": "pinned",
                             "worktree": {"path": "/r/w", "branch": "plan/20261112_02"},
                             "last_decision": {"pane_id": "p1", "decision": "continue", "step": 4,
                                               "reason": "Done.", "confirm_id": 7,
                                               "summary_serial": 9, "at": "2026-11-12T21:30:00-05:00"}}]},
                {"path": "docs/design/20261112_03_B_PLAN.md", "title": "Active plans",
                 "project": {"root": "/r/flankr", "name": "flankr"}, "open_questions": 3,
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
            agent(
                "p1",
                json!({"subagents_running": 1, "focused": true, "idle_ms": 1000,
                "children": [
                    {"id": "a1", "short": "overview screen", "description": "x",
                     "last_activity": "2026-11-14T20:00:00Z"},
                    {"id": "a2", "short": "done one", "description": "y", "finished": true}
                ]}),
            ),
            agent(
                "p2",
                json!({"turn": {"state": "asking", "question": "Shall I go on?"}}),
            ),
            agent("p3", json!({"idle_ms": 5000})),
            agent("p4", json!({"agent": "shell"})),
        ]);
        l.set_overview(Some(overview()), Grouping::ByPlan);
        // A minute after the fixtures' last_activity.
        l.set_wall(wall("2026-11-14T20:01:00Z"));
        l
    }

    fn wall(ts: &str) -> f64 {
        flankr_companion::cross::rfc3339_secs(ts).unwrap()
    }

    /// Opens every **N inactive** section by a tap on it.
    fn open_inactive(l: &mut PlanList) {
        while let Some(r) = l
            .hits()
            .into_iter()
            .find(|(_, h)| {
                matches!(h, ListHit::Fold(k) if k.starts_with("inactive:"))
                    && !l.folded.contains(match h {
                        ListHit::Fold(k) => k,
                        _ => unreachable!(),
                    })
            })
            .map(|(r, _)| r)
        {
            let c = r.center();
            l.touch(77, TouchPhase::Started, c[0], c[1], Duration::ZERO);
            l.touch(77, TouchPhase::Ended, c[0], c[1], Duration::from_millis(40));
        }
    }

    #[test]
    fn headers_say_the_plan_at_a_glance() {
        // The 2026-11-13 01:52 EST amendment: projects, then each plan's
        // row in its stage, its subagents, and no agent row.
        let l = list();
        let t = l.texts();
        assert_eq!(t[0], "flankr", "{t:?}");
        assert!(l.chevrons()[0], "open: the chevron points down");
        let a = t.iter().position(|x| x == "Plan policy agent").unwrap();
        let b = t.iter().position(|x| x == "Active plans").unwrap();
        assert!(
            a < b,
            "the plan an agent moved in most recently first: {t:?}"
        );
        assert!(
            t.contains(
                &"step 3 of 9 \u{b7} claude \u{b7} plan/20261112_02 \u{b7} policy: continue 21:30"
                    .to_string()
            ),
            "{t:?}"
        );
        assert!(
            t.contains(&"Draft \u{b7} claude \u{b7} 3 open questions".to_string()),
            "{t:?}"
        );
        assert!(t.contains(&"overview screen".to_string()), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("done one")), "finished");
        assert!(
            !t.iter().any(|x| x.starts_with("Claude p1")),
            "no agent row under a plan: {t:?}"
        );
        assert!(t.contains(&"Not on a plan".to_string()));
        assert!(t.contains(&"Claude p3".to_string()));
        assert!(!t.iter().any(|x| x.contains("p4")), "no shell");
        // The plan's row opens its agent's view.
        assert!(
            l.hits()
                .iter()
                .any(|(_, h)| *h == ListHit::Agent("p1".into()))
        );
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
        // The question's mark and its Yes, under the plan's row.
        assert!(l.hits().iter().any(|(_, h)| *h
            == ListHit::Answer {
                pane_id: "p2".into(),
                yes: true
            }));
    }

    #[test]
    fn idle_and_drafts_without_agents_are_one_row() {
        let mut o = overview();
        o.plans[1].agents.clear();
        o.plans[0].agents.clear();
        o.plans[0].task = false;
        o.plans[0].current_step = None;
        let mut l = list();
        l.set_overview(Some(o), Grouping::ByPlan);
        // Plans with no agent are inactive: folded until opened.
        assert!(
            l.texts().contains(&"2 inactive".to_string()),
            "{:?}",
            l.texts()
        );
        assert!(
            !l.texts()
                .contains(&"In implementation \u{b7} idle".to_string())
        );
        open_inactive(&mut l);
        let t = l.texts();
        assert!(
            t.contains(&"In implementation \u{b7} idle".to_string()),
            "{t:?}"
        );
        assert!(
            t.contains(&"Draft \u{b7} 3 open questions".to_string()),
            "{t:?}"
        );
        assert!(!l.glyphs().contains(&"\u{2442}".to_string()), "no children");
        // A row with no agent folds.
        assert!(
            l.hits()
                .iter()
                .any(|(_, h)| *h == ListHit::Fold("docs/design/20261112_02_A_PLAN.md".into()))
        );
    }

    #[test]
    fn projects_are_sorted_again_only_after_two_seconds_without_a_touch() {
        let mut o = overview();
        o.plans[1].project = Some(flankr_companion::policy::PolicyProject {
            root: "/r/other".into(),
            name: "other".into(),
        });
        let mut l = list();
        l.set_overview(Some(o), Grouping::ByPlan);
        let first = |l: &PlanList| l.texts()[0].clone();
        assert_eq!(first(&l), "flankr", "p1 moved most recently");
        // A finger down; then p2 moves more recently.
        l.touch(9, TouchPhase::Started, 5.0, 2000.0, Duration::ZERO);
        l.touch(9, TouchPhase::Cancelled, 5.0, 2000.0, Duration::ZERO);
        let mut agents = l.agents.clone();
        agents[1].idle_ms = Some(10);
        l.set_agents(&agents);
        assert_eq!(first(&l), "flankr", "kept under the finger");
        l.tick(RESORT_AFTER);
        assert_eq!(first(&l), "other");
    }

    #[test]
    fn taps_open_the_agent_and_fold_the_project() {
        let mut l = list();
        let row = l
            .hits()
            .into_iter()
            .find(|(_, h)| *h == ListHit::Agent("p1".into()))
            .unwrap()
            .0;
        let c = [row.x + 20.0, row.y + 10.0];
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
            .find(|(_, h)| *h == ListHit::Fold("project:/r/flankr".into()))
            .unwrap()
            .0;
        let c = [fold.x + 20.0, fold.y + 10.0];
        l.touch(2, TouchPhase::Started, c[0], c[1], Duration::ZERO);
        l.touch(2, TouchPhase::Ended, c[0], c[1], Duration::from_millis(50));
        assert!(
            !l.texts().contains(&"Plan policy agent".to_string()),
            "folded"
        );
        assert!(l.texts().contains(&"flankr".to_string()));
        assert!(!l.chevrons()[0], "folded: the chevron points right");
        assert!(l.texts().contains(&"Not on a plan".to_string()));
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

    fn drag(l: &mut PlanList, id: u64, from: [f32; 2], to: [f32; 2]) -> Option<ListHit> {
        l.touch(id, TouchPhase::Started, from[0], from[1], Duration::ZERO);
        let mid = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];
        l.touch(
            id,
            TouchPhase::Moved,
            mid[0],
            mid[1],
            Duration::from_millis(30),
        );
        l.touch(
            id,
            TouchPhase::Moved,
            to[0],
            to[1],
            Duration::from_millis(60),
        );
        l.touch(
            id,
            TouchPhase::Ended,
            to[0],
            to[1],
            Duration::from_millis(90),
        )
    }

    /// The widest area of `hit`: a row's, before a title's arrow.
    fn rect_of(l: &PlanList, hit: &ListHit) -> Rect {
        l.hits()
            .into_iter()
            .filter(|(_, h)| h == hit)
            .max_by(|a, b| a.0.w.total_cmp(&b.0.w))
            .unwrap()
            .0
    }

    /// The 2026-11-13 01:07 EST amendment, item 1.
    #[test]
    fn a_pull_from_the_top_refreshes_until_the_answer() {
        let mut l = list();
        let top = l.viewport().y + 10.0;
        // A short pull: nothing.
        assert_eq!(drag(&mut l, 1, [500.0, top], [500.0, top + 60.0]), None);
        assert!(!l.texts().iter().any(|t| t.contains("refresh")));
        // Past the threshold: a refresh, and its line until the answer.
        assert_eq!(
            drag(&mut l, 2, [500.0, top], [500.0, top + 400.0]),
            Some(ListHit::Refresh)
        );
        assert!(l.texts().contains(&crate::agent::REFRESHING.to_string()));
        l.refreshed();
        assert!(!l.texts().contains(&crate::agent::REFRESHING.to_string()));
        // Or for REFRESH_MAX at most.
        drag(&mut l, 3, [500.0, top], [500.0, top + 400.0]);
        assert!(l.tick(REFRESH_MAX));
        assert!(!l.texts().contains(&crate::agent::REFRESHING.to_string()));
    }

    /// Items 2 and 4.
    #[test]
    fn a_swiped_row_is_hidden_and_never_answered() {
        let mut l = list();
        l.confirm_opened("p1", 7, "continue", "plugin:policy", None);
        let approve = ListHit::Confirm {
            confirm_id: 7,
            approve: true,
        };
        let r = rect_of(&l, &approve);
        // From the row's text, left of its buttons, sideways.
        let start = [r.x + 4.0, r.y - 10.0];
        assert_eq!(
            drag(&mut l, 1, start, [start[0] + 400.0, start[1]]),
            None,
            "no answer"
        );
        assert!(!l.hits().iter().any(|(_, h)| *h == approve), "hidden");
        assert_eq!(l.hidden(), 1);
        assert!(l.texts().iter().any(|t| t == "1 hidden \u{b7} Show"));
        // Its question row stays, and the confirmation is still open.
        assert!(
            l.hits()
                .iter()
                .any(|(_, h)| matches!(h, ListHit::Answer { .. }))
        );
        let show = rect_of(&l, &ListHit::ShowHidden).center();
        l.touch(2, TouchPhase::Started, show[0], show[1], Duration::ZERO);
        assert_eq!(
            l.touch(
                2,
                TouchPhase::Ended,
                show[0],
                show[1],
                Duration::from_millis(40)
            ),
            Some(ListHit::ShowHidden)
        );
        assert!(
            l.hits().iter().any(|(_, h)| *h == approve),
            "back, still open"
        );
    }

    /// Item 3.
    #[test]
    fn a_swiped_task_comes_back_on_a_new_event_or_a_state_change() {
        let mut l = list();
        let path = "docs/design/20261112_02_A_PLAN.md".to_string();
        let r = rect_of(&l, &ListHit::Agent("p1".into()));
        let start = [r.x + 30.0, r.y + 10.0];
        assert_eq!(drag(&mut l, 1, start, [start[0] - 400.0, start[1]]), None);
        assert!(
            !l.texts().contains(&"Plan policy agent".to_string()),
            "hidden"
        );
        // Only the project title's arrow still opens p1.
        assert!(
            !l.hits()
                .iter()
                .any(|(r, h)| *h == ListHit::Agent("p1".into()) && r.w > 48.0 * 2.0)
        );
        assert_eq!(l.hidden(), 1);
        // The same state again: still hidden.
        l.set_overview(Some(overview()), Grouping::ByPlan);
        assert!(!l.texts().contains(&"Plan policy agent".to_string()));
        // A new event under its agent brings it back.
        l.confirm_opened("p1", 8, "continue", "plugin:policy", None);
        assert!(l.texts().contains(&"Plan policy agent".to_string()));
        // Hidden again; a state change (its step) brings it back.
        let r = rect_of(&l, &ListHit::Agent("p1".into()));
        drag(
            &mut l,
            2,
            [r.x + 30.0, r.y + 10.0],
            [r.x + 430.0, r.y + 10.0],
        );
        assert!(!l.texts().contains(&"Plan policy agent".to_string()));
        let mut next = overview();
        next.plans[0].current_step = Some(4);
        l.set_overview(Some(next), Grouping::ByPlan);
        assert!(l.texts().contains(&"Plan policy agent".to_string()));
        // Hiding never turns autocontinue off: its glyph is as it was.
        assert!(l.hits().iter().any(|(_, h)| *h
            == ListHit::Autocontinue {
                path: path.clone(),
                on: false
            }));
    }

    /// The active tasks overview plan, 2.1 and step 1: each kind's glyph
    /// is on the terminal face's symbol page, so no missing-glyph box.
    #[test]
    fn every_kind_glyph_is_on_the_symbol_page() {
        use libmsdf::font::{FontAtlas, NOTO_SANS_MONO_ATLAS, TERMINAL_SYMBOLS_ATLAS};
        use libmsdf::font::{GlyphStyle, TerminalShaper, TerminalSource};
        let text = FontAtlas::from_bytes(NOTO_SANS_MONO_ATLAS).expect("parses");
        let page = FontAtlas::from_bytes(TERMINAL_SYMBOLS_ATLAS).expect("parses");
        let shaper = TerminalShaper::new(&text, &page).expect("the atlases agree");
        let kinds = [Kind::Plan, Kind::Subagent, Kind::Autocontinue, Kind::Goal];
        assert_eq!(
            kinds.map(Kind::glyph),
            ["\u{2611}", "\u{2442}", "\u{21bb}", "\u{25ce}"]
        );
        for k in kinds {
            let line = shaper.line(k.glyph(), GlyphStyle::Regular);
            assert!(
                line.segments
                    .iter()
                    .all(|g| g.source == TerminalSource::Symbols
                        && g.run.glyphs.iter().all(|x| x.glyph_id != 0)),
                "{k:?} is on the symbol page"
            );
        }
    }

    /// 2.1 items 1 and 3: ☑ leads each plan's row, ⑂ each subagent's; ↻
    /// follows a task's title while its autocontinue is on, ◎ a plan an
    /// agent's goal names; no ▸, ▾ or ↳ anywhere.
    #[test]
    fn each_row_starts_with_its_kind() {
        let l = list();
        let g = l.glyphs();
        assert_eq!(
            g,
            vec!["\u{2611}", "\u{21bb}", "\u{2442}", "\u{2611}"],
            "the task's ☑ and ↻, its subagent's ⑂, the draft's ☑"
        );
        let mut o = overview();
        o.plans[0].autocontinue = false;
        o.plans[1].agents[0].linked_by = "goal".into();
        let mut l = list();
        l.set_overview(Some(o.clone()), Grouping::ByPlan);
        assert_eq!(
            l.glyphs(),
            vec!["\u{2611}", "\u{2442}", "\u{2611}", "\u{25ce}"],
            "no ↻ with autocontinue off; ◎ for the goal's plan"
        );
        // Both marks when both hold.
        o.plans[0].autocontinue = true;
        o.plans[0].agents[0].linked_by = "goal".into();
        l.set_overview(Some(o), Grouping::ByPlan);
        assert_eq!(&l.glyphs()[..3], ["\u{2611}", "\u{21bb}", "\u{25ce}"]);
        for t in l.texts() {
            assert!(
                !t.contains(['\u{25b8}', '\u{25be}', '\u{21b3}']),
                "no character chevron or arrow: {t:?}"
            );
        }
        // Drawn: the chevron's two arms are lines, the glyphs in the face.
        let mut dl = DrawList::new();
        l.draw(&mut dl, &DrawContext::default(), None);
        assert!(!dl.instances.is_empty());
    }

    /// The active tasks overview plan, step 3: a working row's glyph is
    /// the drawer's spinner frame at the same moment, and with nothing
    /// working the list is not drawn again at the spinner's rate.
    #[test]
    fn a_working_row_spins_with_the_drawer() {
        use crate::drawer::spinner::{FRAME, glyph_at};
        let mut l = list();
        let ms = Duration::from_millis;
        // Nothing working (the fixture agents are idle, from a daemon
        // that sends last_activity): no spinner, no redraws.
        let mut agents = l.agents.clone();
        for a in &mut agents {
            a.last_activity = Some("2026-11-14T20:00:00Z".into());
        }
        agents[0].children[0].working = false;
        agents[0].children[0].last_activity = Some("2026-11-14T20:00:00Z".into());
        l.set_agents(&agents);
        l.take_dirty();
        assert!(!l.spinning());
        for i in 0..5 {
            l.set_clock(FRAME * i);
            assert!(!l.take_dirty(), "no redraw while nothing spins");
        }
        // p1 working, and its subagent: the plan's ☑ and the ⑂ spin.
        agents[0].working = true;
        agents[0].children[0].working = true;
        l.set_agents(&agents);
        let t = ms(80 * 7 + 3);
        l.set_clock(t);
        assert!(l.spinning());
        let g = l.glyphs();
        assert_eq!(g[0], glyph_at(t).to_string(), "the drawer's frame: {g:?}");
        assert_eq!(g[2], glyph_at(t).to_string(), "the subagent's too");
        assert_eq!(g[1], "\u{21bb}", "the mark stays");
        assert!(l.take_dirty());
        l.set_clock(t + ms(10));
        assert!(!l.take_dirty(), "the same frame: nothing new");
        l.set_clock(t + FRAME);
        assert!(l.take_dirty(), "the next frame");
        // An older daemon (no last_activity): herdr's working.
        let mut old = agent("p9", json!({"status": "working"}));
        assert!(agent_works(&old));
        old.status = "idle".into();
        assert!(!agent_works(&old));
        // A goal naming no plan: its own ◎ row, spinning while it works.
        agents[2].goal = Some("tidy the docs\nmore".into());
        agents[2].last_activity = Some("2026-11-14T20:00:00Z".into());
        l.set_agents(&agents);
        assert!(
            l.texts().contains(&"tidy the docs".to_string()),
            "{:?}",
            l.texts()
        );
        assert!(l.glyphs().contains(&"\u{25ce}".to_string()));
        assert!(
            l.hits()
                .iter()
                .any(|(_, h)| *h == ListHit::Agent("p3".into()))
        );
    }

    /// The active tasks overview plan, step 4: an agent idle for more
    /// than 10 minutes moves into its group's closed **N inactive**
    /// section; one with a question waiting never does; one that starts
    /// working again leaves it at once.
    #[test]
    fn quiet_rows_fold_under_n_inactive() {
        let mut l = list();
        let mut agents = l.agents.clone();
        // p3 (not on a plan) last produced 11 minutes ago.
        agents[2].idle_ms = None;
        agents[2].last_activity = Some("2026-11-14T19:50:00Z".into());
        l.set_agents(&agents);
        let t = l.texts();
        assert!(!t.contains(&"Claude p3".to_string()), "{t:?}");
        assert_eq!(t.last().map(String::as_str), Some("1 inactive"));
        assert!(!*l.chevrons().last().unwrap(), "closed: pointing right");
        // Opened by a tap; closed again by another.
        open_inactive(&mut l);
        assert!(l.texts().contains(&"Claude p3".to_string()));
        assert!(*l.chevrons().last().unwrap(), "open: pointing down");
        let r = rect_of(&l, &ListHit::Fold("inactive:".into())).center();
        l.touch(5, TouchPhase::Started, r[0], r[1], Duration::ZERO);
        l.touch(5, TouchPhase::Ended, r[0], r[1], Duration::from_millis(40));
        assert!(!l.texts().contains(&"Claude p3".to_string()));
        // 9 minutes: still active.
        l.set_wall(wall("2026-11-14T19:59:00Z"));
        assert!(l.texts().contains(&"Claude p3".to_string()));
        l.set_wall(wall("2026-11-14T20:01:00Z"));
        // Working again: out of the section at once.
        agents[2].working = true;
        l.set_agents(&agents);
        assert!(l.texts().contains(&"Claude p3".to_string()));
        assert!(!l.texts().iter().any(|t| t.ends_with("inactive")));
        // A question waiting: never inactive, however old.
        agents[2].working = false;
        agents[2].turn = Some(Turn::Asking {
            question: "Merge it?".into(),
        });
        l.set_agents(&agents);
        assert!(
            l.texts().contains(&"Claude p3".to_string()),
            "{:?}",
            l.texts()
        );
        // A plan whose agents went quiet folds too, its questions aside:
        // p2 asks, so its plan stays; p1's plan goes once p1 is quiet.
        agents[2].turn = None;
        agents[0].idle_ms = None;
        agents[0].last_activity = Some("2026-11-14T19:00:00Z".into());
        l.set_agents(&agents);
        let t = l.texts();
        assert!(!t.contains(&"Plan policy agent".to_string()), "{t:?}");
        assert!(t.contains(&"Active plans".to_string()), "p2 is asking");
        // A confirmation open on p1's pane brings its plan back.
        l.confirm_opened("p1", 30, "continue", "plugin:policy", None);
        assert!(l.texts().contains(&"Plan policy agent".to_string()));
    }

    /// The active tasks overview plan, step 5: a tap on the project
    /// title's arrow opens its most recently active top-level agent; a
    /// tap elsewhere on the title folds it; no agent, no arrow.
    #[test]
    fn the_titles_arrow_opens_the_projects_main_agent() {
        let mut l = list();
        let mut agents = l.agents.clone();
        agents[0].last_activity = Some("2026-11-14T20:00:00Z".into());
        agents[1].last_activity = Some("2026-11-14T20:00:30Z".into());
        l.set_agents(&agents);
        let title = rect_of(&l, &ListHit::Fold("project:/r/flankr".into()));
        let arrow = l
            .hits()
            .into_iter()
            .find(|(r, h)| matches!(h, ListHit::Agent(_)) && r.y == title.y)
            .unwrap();
        assert_eq!(arrow.1, ListHit::Agent("p2".into()), "p2 the newest");
        assert_eq!(arrow.0.w, 48.0 * 2.0, "48 pt at scale 2");
        assert_eq!(
            arrow.0.x + arrow.0.w,
            title.x + title.w,
            "at the right edge"
        );
        assert_eq!(arrow.0.h, title.h, "its full height");
        assert_eq!(
            l.chevrons()[..2],
            [true, false],
            "the fold's, then the arrow"
        );
        let tap = |l: &mut PlanList, c: [f32; 2]| {
            l.touch(8, TouchPhase::Started, c[0], c[1], Duration::ZERO);
            l.touch(8, TouchPhase::Ended, c[0], c[1], Duration::from_millis(40))
        };
        assert_eq!(
            tap(&mut l, arrow.0.center()),
            Some(ListHit::Agent("p2".into()))
        );
        assert!(
            l.texts().contains(&"Plan policy agent".to_string()),
            "not folded"
        );
        let left = [title.x + 40.0, title.y + title.h * 0.5];
        assert_eq!(
            tap(&mut l, left),
            Some(ListHit::Fold("project:/r/flankr".into()))
        );
        assert!(
            !l.texts().contains(&"Plan policy agent".to_string()),
            "folded"
        );
        // A project with no agent: no arrow.
        let mut o = overview();
        o.plans[0].agents.clear();
        o.plans[1].agents.clear();
        let mut l = list();
        l.set_overview(Some(o), Grouping::ByPlan);
        let title = rect_of(&l, &ListHit::Fold("project:/r/flankr".into()));
        assert!(
            !l.hits()
                .iter()
                .any(|(r, h)| matches!(h, ListHit::Agent(_)) && r.y == title.y)
        );
        assert!(l.chevrons()[0]);
        assert_eq!(
            l.chevrons().len(),
            2,
            "the fold's and the inactive section's"
        );
    }
}
