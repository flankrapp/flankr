//! What the drawer's sheet holds (plan 20261028_03, 3.1 and 3.11): the
//! remote cards, 4 across and up to 2 rows, with the **+** after the last
//! when fewer than 8 are paired, then the remote in use's sessions in a
//! libcontrols `ScrollPane`, one [`SessionRow`] each, in `agents.list`
//! order. Built from the client's events into the drawer's content router,
//! in the sheet's own coordinates.
//!
//! Grouped by workspace (plan 20261103_02, decisions 3.1 to 3.5; the
//! default), the list is in herdr's workspace order instead: a
//! [`GroupHeader`] per workspace with two rows or more, its agent sessions
//! in tab order and then its shell panes under it unless it is collapsed;
//! a workspace with one row has no header, and its row, in the place the
//! header would have had, shows its full name (the 13:05 EST amendment,
//! item 3). Not grouped, the list is as above. The collapsed workspaces
//! are the remote in use's, from [`RemoteInfo::collapsed_workspaces`]; a
//! change is taken with [`DrawerContent::take_collapsed`] for the client
//! to keep.
//!
//! [`DrawerContent::build`] makes the nodes again when what they show
//! changes (a list, the remotes, a pending approval, the view open);
//! between lists [`DrawerContent::refresh`] judges each row's activity
//! again at the time given, so a row stops spinning [`RECENT`] after its
//! last write, and says when the sheet must be drawn again (the spinner's
//! frame included). The list's `ScrollState` is kept across rebuilds.
//!
//! [`RECENT`]: flankr_companion::activity::RECENT

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use flankr_companion::activity::{self, Activity, Signals};
use flankr_companion::{MAX_REMOTES, RemoteId, RemoteInfo};
use libcontrols::{
    ControlAction, ControlPart, DrawContext, DrawControl, EventContext, HostedControl,
    KeyboardEvent, MouseEvent, NodeId, Rect, Router, ScrollAxis, ScrollPane, ScrollState,
};
use libflankr_proto::common::AgentInfo;
use libmsdf::drawlist::DrawList;

use super::group_header::{GroupHeader, HEADER_H};
use super::remote_card::{CARD_H, RemoteCard, Slot};
use super::session_row::{COLUMN_W, ROW_H, SessionRow};
use super::{DrawerHit, cut_right, spinner};
use crate::agent::line_h;
use crate::settings::copy;
use crate::theme::theme;

/// The sheet's padding, the gap between cards and below the grid, in
/// points; the cards across.
pub const PAD: f32 = 12.0;
pub const CARD_GAP: f32 = 8.0;
pub const COLUMNS: usize = 4;
const NOTE_PT: f32 = 14.0;

/// The list's one row while it has no sessions to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// Its connection failed; the next try in this many seconds.
    Unreachable(u32),
    /// Connected, and no sessions.
    NoSessions,
}

/// The card slots' rects for `n` remotes on a sheet `w` wide, from `top`,
/// at scale `s`: 4 across with 8 pt gaps, filling the first row then the
/// second, the **+** after the last when fewer than 8.
pub fn grid(w: f32, top: f32, n: usize, s: f32) -> Vec<Rect> {
    let slots = n.min(MAX_REMOTES) + usize::from(n < MAX_REMOTES);
    let pad = PAD * s;
    let gap = CARD_GAP * s;
    let cw = (w - 2.0 * pad - (COLUMNS as f32 - 1.0) * gap) / COLUMNS as f32;
    let ch = CARD_H * s;
    (0..slots)
        .map(|i| {
            let (row, col) = (i / COLUMNS, i % COLUMNS);
            Rect::new(
                pad + col as f32 * (cw + gap),
                top + row as f32 * (ch + gap),
                cw,
                ch,
            )
        })
        .collect()
}

/// One workspace's sessions, in the grouped drawer.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// `None` for sessions with no workspace (an older daemon): listed
    /// after the groups, as in the flat list, with no header.
    pub workspace_id: Option<String>,
    /// The header's text.
    pub text: String,
    /// herdr's workspace number.
    pub number: Option<u32>,
    /// Indexes into the list: agent sessions in tab order, then shell
    /// panes in tab order.
    pub rows: Vec<usize>,
}

impl Group {
    /// Whether it has a header: a workspace with two rows or more.
    pub fn headed(&self) -> bool {
        self.workspace_id.is_some() && self.rows.len() > 1
    }
}

/// What the list shows, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// A workspace's header: the group's index.
    Header(usize),
    /// A session's row: its index in the list, and whether it stands
    /// alone for its workspace, with its full name.
    Row { index: usize, lone: bool },
}

/// The header's text for workspace `n` with no label and no project.
fn workspace_text(number: Option<u32>) -> String {
    match number {
        Some(n) => copy::DRAWER_WORKSPACE_NUMBER.fill(None, &[("number", &n.to_string())]),
        None => copy::DRAWER_WORKSPACE.text.to_string(),
    }
    .trim()
    .to_string()
}

/// The note's one row, dim, in the list.
#[derive(Clone, Debug)]
pub struct NoteRow {
    pub rect: Rect,
    pub text: String,
    pub scale: f32,
}

impl DrawControl for NoteRow {
    fn draw(&self, list: &mut DrawList, ctx: &DrawContext) {
        let s = self.scale;
        let px = NOTE_PT * s;
        let x = self.rect.x + 4.0 * s;
        let room = self.rect.w - 8.0 * s;
        let text = cut_right(ctx, &self.text, px, room);
        let y = self.rect.y + (self.rect.h - line_h(px)) * 0.5;
        ctx.draw_text(list, &text, [x, y], px, theme().dim);
    }
}

/// It takes no taps.
impl HostedControl for NoteRow {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit_test(&self, _pos: [f32; 2]) -> Option<(ControlPart, f32)> {
        None
    }

    fn handle_mouse(&self, _event: &MouseEvent, _ctx: &EventContext) -> Option<ControlAction> {
        None
    }

    fn handle_keyboard(
        &self,
        _event: &KeyboardEvent,
        _ctx: &EventContext,
    ) -> Option<ControlAction> {
        None
    }
}

/// The sheet's controls, as data and as built.
#[derive(Debug)]
pub struct DrawerContent {
    remotes: Vec<RemoteInfo>,
    /// Background remotes with a question or an approval waiting.
    waiting: HashSet<RemoteId>,
    agents: Vec<AgentInfo>,
    /// A list has been given (kept or the daemon's), since the last
    /// [`DrawerContent::unload`].
    listed: bool,
    /// When the list came, on the app's clock.
    received_at: Duration,
    pending: Vec<String>,
    open_pane: Option<String>,
    /// The next try, while the remote in use cannot be reached.
    retry_in: Option<u32>,
    /// *Drawer sessions*: grouped by workspace, or not.
    grouped: bool,
    /// The remote in use's collapsed workspaces, by id, and which remote
    /// they are for.
    collapsed: HashSet<String>,
    collapsed_for: Option<RemoteId>,
    /// The collapsed workspaces changed here, not yet taken.
    collapsed_changed: bool,
    /// They were changed here for this remote: the client's echo of them
    /// is not taken back.
    collapsed_own: bool,
    /// The panes with a question or an approval waiting, as last judged,
    /// once a list has been given (decision 3.5).
    waiting_panes: Option<HashSet<String>>,
    /// The sheet's size, the safe area's top and bottom, the scale.
    size: [f32; 2],
    insets: [f32; 2],
    scale: f32,
    scroll: Arc<ScrollState>,
    /// As built: the cards' rects and slots, the list's viewport, its pane
    /// and its rows' nodes.
    cards: Vec<(Rect, Slot, NodeId)>,
    list: Rect,
    pane: Option<NodeId>,
    rows: Vec<(NodeId, String)>,
    /// The headers' nodes and their workspace ids.
    headers: Vec<(NodeId, String)>,
    /// The card held.
    press: Option<usize>,
    /// The spinner's frame last drawn, while any row spins.
    frame: Option<usize>,
    /// Something it shows changed: built again before it is drawn.
    dirty: bool,
}

impl Default for DrawerContent {
    fn default() -> Self {
        DrawerContent {
            remotes: Vec::new(),
            waiting: HashSet::new(),
            agents: Vec::new(),
            listed: false,
            received_at: Duration::ZERO,
            pending: Vec::new(),
            open_pane: None,
            retry_in: None,
            grouped: true,
            collapsed: HashSet::new(),
            collapsed_for: None,
            collapsed_changed: false,
            collapsed_own: false,
            waiting_panes: None,
            size: [0.0; 2],
            insets: [0.0; 2],
            scale: 1.0,
            scroll: ScrollState::new(),
            cards: Vec::new(),
            list: Rect::new(0.0, 0.0, 0.0, 0.0),
            pane: None,
            rows: Vec::new(),
            headers: Vec::new(),
            press: None,
            frame: None,
            dirty: true,
        }
    }
}

impl DrawerContent {
    pub fn set_remotes(&mut self, remotes: Vec<RemoteInfo>) {
        if remotes != self.remotes {
            self.remotes = remotes;
            self.dirty = true;
        }
        self.adopt_collapsed();
    }

    /// The remote in use's collapsed workspaces, as the client keeps them:
    /// all of them when the remote in use is another, and while none were
    /// changed here; a change made here is not undone by the client's
    /// echo of an older one.
    fn adopt_collapsed(&mut self) {
        let Some(active) = self.remotes.iter().find(|r| r.active) else {
            return;
        };
        let kept: HashSet<String> = active.collapsed_workspaces.iter().cloned().collect();
        if self.collapsed_for != Some(active.id) {
            self.collapsed_for = Some(active.id);
            self.collapsed = kept;
            self.collapsed_changed = false;
            self.collapsed_own = false;
            // Another remote's sessions: their waiting is judged afresh.
            self.waiting_panes = None;
            self.dirty = true;
        } else if !self.collapsed_own && kept != self.collapsed {
            self.collapsed = kept;
            self.dirty = true;
        }
    }

    /// *Drawer sessions*: grouped by workspace, or the flat list.
    pub fn set_grouped(&mut self, grouped: bool) {
        if grouped != self.grouped {
            self.grouped = grouped;
            self.dirty = true;
        }
    }

    pub fn grouped(&self) -> bool {
        self.grouped
    }

    /// Whether workspace `id`'s group is kept collapsed.
    pub fn is_collapsed(&self, id: &str) -> bool {
        self.collapsed.contains(id)
    }

    /// A tap on workspace `id`'s header: collapses its group, or expands
    /// it.
    pub fn toggle_group(&mut self, id: &str) {
        if !self.collapsed.remove(id) {
            self.collapsed.insert(id.to_string());
        }
        self.collapsed_changed = true;
        self.dirty = true;
    }

    /// The collapsed workspaces when they changed here since last taken,
    /// with the remote they are for, sorted: for the client to keep in the
    /// remote's `state.redb`.
    pub fn take_collapsed(&mut self) -> Option<(RemoteId, Vec<String>)> {
        if !std::mem::take(&mut self.collapsed_changed) {
            return None;
        }
        let id = self.collapsed_for?;
        self.collapsed_own = true;
        let mut list: Vec<String> = self.collapsed.iter().cloned().collect();
        list.sort();
        Some((id, list))
    }

    /// Whether `agent` has a question or an approval waiting.
    fn waits(&self, agent: &AgentInfo) -> bool {
        matches!(
            self.activity_of(agent, self.received_at),
            Activity::NeedsInput(_)
        )
    }

    /// The group `pane` is in, if it has a header and is collapsed, is
    /// expanded (decision 3.5).
    fn expand_for(&mut self, pane: &str) {
        let Some(group) = self
            .groups()
            .into_iter()
            .find(|g| g.rows.iter().any(|&i| self.agents[i].pane_id == pane))
        else {
            return;
        };
        if let (true, Some(id)) = (group.headed(), &group.workspace_id)
            && self.collapsed.remove(id)
        {
            self.collapsed_changed = true;
            self.dirty = true;
        }
    }

    /// A session that starts waiting is never hidden by a collapse made
    /// before (decision 3.5): its group expands. The first list after a
    /// start or a switch only sets what is waiting, so a group collapsed
    /// over a waiting session stays collapsed, with its dot.
    fn follow_waiting(&mut self) {
        if !self.listed {
            return;
        }
        let now: HashSet<String> = self
            .agents
            .iter()
            .filter(|a| self.waits(a))
            .map(|a| a.pane_id.clone())
            .collect();
        if let Some(before) = self.waiting_panes.take() {
            for pane in now.difference(&before) {
                self.expand_for(pane);
            }
        }
        self.waiting_panes = Some(now);
    }

    /// The list's groups by workspace, in herdr's order: by workspace
    /// number, then those with none by their header's text; sessions with
    /// no workspace last.
    pub fn groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        for (i, a) in self.agents.iter().enumerate() {
            match groups.iter_mut().find(|g| g.workspace_id == a.workspace_id) {
                Some(g) => {
                    g.rows.push(i);
                    g.number = g.number.or(a.workspace_number);
                }
                None => groups.push(Group {
                    workspace_id: a.workspace_id.clone(),
                    text: String::new(),
                    number: a.workspace_number,
                    rows: vec![i],
                }),
            }
        }
        for g in &mut groups {
            let agents = &self.agents;
            g.rows.sort_by_key(|&i| {
                let a = &agents[i];
                (
                    flankr_companion::cross::is_terminal_only(a),
                    a.tab_number.unwrap_or(u32::MAX),
                )
            });
            let first = |f: fn(&AgentInfo) -> Option<&String>| {
                g.rows
                    .iter()
                    .find_map(|&i| f(&agents[i]).filter(|t| !t.trim().is_empty()))
                    .cloned()
            };
            g.text = first(|a| a.workspace_label.as_ref())
                .or_else(|| first(|a| a.project.as_ref()))
                .unwrap_or_else(|| workspace_text(g.number));
        }
        groups.sort_by(|a, b| {
            (a.workspace_id.is_none(), a.number.is_none(), a.number)
                .cmp(&(b.workspace_id.is_none(), b.number.is_none(), b.number))
                .then_with(|| match (a.number, b.number) {
                    (None, None) => a.text.to_lowercase().cmp(&b.text.to_lowercase()),
                    _ => std::cmp::Ordering::Equal,
                })
        });
        groups
    }

    /// What the list shows, in order: grouped, the headers and the rows
    /// not collapsed; not grouped, every row in the list's order.
    pub fn entries(&self) -> (Vec<Group>, Vec<Entry>) {
        if !self.grouped {
            let rows = (0..self.agents.len())
                .map(|index| Entry::Row { index, lone: false })
                .collect();
            return (Vec::new(), rows);
        }
        let groups = self.groups();
        let mut entries = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            if !g.headed() {
                let lone = g.workspace_id.is_some();
                entries.extend(g.rows.iter().map(|&index| Entry::Row { index, lone }));
                continue;
            }
            entries.push(Entry::Header(gi));
            if g.workspace_id
                .as_ref()
                .is_some_and(|id| self.collapsed.contains(id))
            {
                continue;
            }
            entries.extend(
                g.rows
                    .iter()
                    .map(|&index| Entry::Row { index, lone: false }),
            );
        }
        (groups, entries)
    }

    pub fn remotes(&self) -> &[RemoteInfo] {
        &self.remotes
    }

    /// The remote in use, if one is.
    pub fn active(&self) -> Option<&RemoteInfo> {
        self.remotes.iter().find(|r| r.active)
    }

    /// The background remotes whose cards show a dot.
    pub fn waiting(&self) -> &HashSet<RemoteId> {
        &self.waiting
    }

    pub fn set_waiting(&mut self, waiting: HashSet<RemoteId>) {
        if waiting != self.waiting {
            self.waiting = waiting;
            self.dirty = true;
        }
    }

    /// A list of the remote in use's agents, received at `now`: its agent
    /// sessions in the list's order, then its shell panes in theirs (plan
    /// 20261103_01, decision 7).
    pub fn set_agents(&mut self, agents: Vec<AgentInfo>, now: Duration) {
        let (sessions, shells): (Vec<AgentInfo>, Vec<AgentInfo>) = agents
            .into_iter()
            .partition(|a| !flankr_companion::cross::is_terminal_only(a));
        self.agents = sessions;
        self.agents.extend(shells);
        self.listed = true;
        self.received_at = now;
        self.dirty = true;
        self.follow_waiting();
    }

    /// No list: the rows go and no note shows until one is given (a switch
    /// to a remote with none kept).
    pub fn unload(&mut self) {
        self.agents.clear();
        self.listed = false;
        self.waiting_panes = None;
        self.dirty = true;
    }

    pub fn agents(&self) -> &[AgentInfo] {
        &self.agents
    }

    pub fn set_pending(&mut self, pending: Vec<String>) {
        if pending != self.pending {
            self.pending = pending;
            self.dirty = true;
            self.follow_waiting();
        }
    }

    /// The pane whose agent view is open under the drawer. A group
    /// collapsed over it is expanded, so it is never hidden by a collapse
    /// made before it opened (decision 3.5).
    pub fn set_open_pane(&mut self, pane: Option<String>) {
        if pane != self.open_pane {
            if let Some(p) = &pane {
                self.expand_for(p);
            }
            self.open_pane = pane;
            self.dirty = true;
        }
    }

    /// The remote in use's connection failed; the next try in `retry_in`
    /// seconds (`None`: connected, or connecting).
    pub fn set_retry(&mut self, retry_in: Option<u32>) {
        if retry_in != self.retry_in {
            self.retry_in = retry_in;
            self.dirty = true;
        }
    }

    /// The sheet's size and the safe area's insets, at scale `s`.
    pub fn set_frame(&mut self, size: [f32; 2], insets: [f32; 2], s: f32) {
        if size != self.size || insets != self.insets || s != self.scale {
            self.size = size;
            self.insets = insets;
            self.scale = s;
            self.dirty = true;
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Marks it to be built again (a theme change).
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// The list's one note row, if it shows one: none while there are
    /// rows, and no looking-for message (plan 20261028_03, the 2026-10-30
    /// 13:15 EDT amendment): with no rows, a failed connection's next
    /// try, or *No sessions* once a list came empty.
    pub fn note(&self) -> Option<Note> {
        let active = self.active()?;
        if !self.agents.is_empty() {
            return None;
        }
        if !active.online {
            return self.retry_in.map(Note::Unreachable);
        }
        self.listed.then_some(Note::NoSessions)
    }

    /// The note's words.
    pub fn note_text(&self, note: &Note) -> String {
        let name = self
            .active()
            .map(|r| r.display_name.as_str())
            .unwrap_or_default();
        let text = match note {
            Note::Unreachable(secs) => copy::DRAWER_UNREACHABLE
                .fill(None, &[("nickname", name), ("seconds", &secs.to_string())]),
            Note::NoSessions => copy::DRAWER_NO_SESSIONS.fill(None, &[("nickname", name)]),
        };
        text.trim().to_string()
    }

    /// `agent`'s activity at `now`.
    pub fn activity_of(&self, agent: &AgentInfo, now: Duration) -> Activity {
        let approval = self.pending.contains(&agent.pane_id);
        activity::activity(agent, approval, Signals::of(agent, self.received_at), now)
    }

    /// Makes the content router's nodes for what it shows, at `now`.
    pub fn build(&mut self, router: &mut Router, now: Duration) {
        let s = self.scale;
        let [w, h] = self.size;
        let pad = PAD * s;
        *router = Router::new();
        let top = self.insets[0] + pad;
        let rects = grid(w, top, self.remotes.len(), s);
        let mut slots: Vec<Slot> = self
            .remotes
            .iter()
            .take(MAX_REMOTES)
            .cloned()
            .map(Slot::Remote)
            .collect();
        if slots.len() < rects.len() {
            slots.push(Slot::Add);
        }
        self.cards = rects
            .iter()
            .zip(slots)
            .enumerate()
            .map(|(i, (&rect, slot))| {
                let waiting =
                    matches!(&slot, Slot::Remote(r) if !r.active && self.waiting.contains(&r.id));
                let mut card = RemoteCard::new(rect, slot.clone(), waiting, s);
                card.pressed = self.press == Some(i);
                let id = router.add(None, card);
                (rect, slot, id)
            })
            .collect();
        let grid_bottom = rects.last().map_or(top, |r| r.y + r.h);
        let list_top = grid_bottom + pad;
        let list_bottom = (h - self.insets[1] - pad).max(list_top);
        self.list = Rect::new(pad, list_top, w - 2.0 * pad, list_bottom - list_top);
        let pane = router.add(
            None,
            ScrollPane::new(self.list, ScrollAxis::Vertical).with_state(Arc::clone(&self.scroll)),
        );
        self.pane = Some(pane);
        self.rows.clear();
        let row_h = ROW_H * s;
        let mut y = list_top;
        if let Some(note) = self.note() {
            let text = self.note_text(&note);
            router.add(
                Some(pane),
                NoteRow {
                    rect: Rect::new(self.list.x, y, self.list.w, row_h),
                    text,
                    scale: s,
                },
            );
            y += row_h;
        }
        // The rows show online or not: a kept list shows at once (the
        // 13:15 EDT amendment, item 2).
        self.headers.clear();
        let (groups, entries) = self.entries();
        for entry in entries {
            match entry {
                Entry::Header(gi) => {
                    let g = &groups[gi];
                    let id = g.workspace_id.clone().unwrap_or_default();
                    let header_h = HEADER_H * s;
                    let header = GroupHeader {
                        rect: Rect::new(self.list.x, y, self.list.w, header_h),
                        collapsed: self.collapsed.contains(&id),
                        waiting: g.rows.iter().any(|&i| self.waits(&self.agents[i])),
                        workspace_id: id.clone(),
                        text: g.text.clone(),
                        count: g.rows.len(),
                        scale: s,
                    };
                    let node = router.add(Some(pane), header);
                    self.headers.push((node, id));
                    y += header_h;
                }
                Entry::Row { index, lone } => {
                    let agent = &self.agents[index];
                    let approval = self.pending.contains(&agent.pane_id);
                    let open = self.open_pane.as_deref() == Some(agent.pane_id.as_str());
                    let mut row =
                        SessionRow::new(agent, approval, self.activity_of(agent, now), open, s);
                    if lone {
                        // No header names its workspace: its full name.
                        row.tab = flankr_companion::cross::shown_name(agent);
                    }
                    row.rect = Rect::new(self.list.x, y, self.list.w, row_h);
                    let id = router.add(Some(pane), row);
                    self.rows.push((id, agent.pane_id.clone()));
                    y += row_h;
                }
            }
        }
        if let Some(p) = router.get::<ScrollPane>(pane) {
            p.set_content_size(Some([self.list.w, y - list_top]));
        }
        self.frame = self.spinning(router).then(|| spinner::frame_at(now));
        self.dirty = false;
    }

    /// Whether any row spins.
    fn spinning(&self, router: &Router) -> bool {
        self.rows.iter().any(|(id, _)| {
            router
                .get::<SessionRow>(*id)
                .is_some_and(|r| r.activity == Activity::Processing)
        })
    }

    /// Judges each row again at `now`, in place. Whether the sheet must be
    /// drawn again: a row's activity or age changed, or the spinner's
    /// frame moved on.
    pub fn refresh(&mut self, router: &mut Router, now: Duration) -> bool {
        let mut changed = false;
        for (id, pane) in &self.rows {
            let Some(agent) = self.agents.iter().find(|a| &a.pane_id == pane) else {
                continue;
            };
            let activity = self.activity_of(agent, now);
            if let Some(row) = router.get_mut::<SessionRow>(*id) {
                if row.indicator() != super::session_row::indicator(activity) {
                    changed = true;
                }
                row.activity = activity;
            }
        }
        let frame = self.spinning(router).then(|| spinner::frame_at(now));
        if frame != self.frame {
            self.frame = frame;
            changed = true;
        }
        changed
    }

    /// The card slot at `pos`, in sheet coordinates.
    pub fn card_at(&self, pos: [f32; 2]) -> Option<usize> {
        self.cards.iter().position(|(r, ..)| r.contains(pos))
    }

    /// What a tap on card `i` is.
    pub fn card_hit(&self, i: usize) -> Option<DrawerHit> {
        match &self.cards.get(i)?.1 {
            Slot::Remote(r) => Some(DrawerHit::Remote(r.id)),
            Slot::Add => Some(DrawerHit::AddRemote),
        }
    }

    /// Where card `i` is, in sheet coordinates.
    pub fn card_rect(&self, i: usize) -> Option<Rect> {
        self.cards.get(i).map(|(r, ..)| *r)
    }

    /// The cards' slots, in order.
    pub fn slots(&self) -> Vec<Slot> {
        self.cards.iter().map(|(_, s, _)| s.clone()).collect()
    }

    /// The card held, shown in the press colour in place.
    pub fn set_press(&mut self, router: &mut Router, press: Option<usize>) -> bool {
        if press == self.press {
            return false;
        }
        self.press = press;
        for (i, (_, _, id)) in self.cards.iter().enumerate() {
            if let Some(card) = router.get_mut::<RemoteCard>(*id) {
                card.pressed = press == Some(i);
            }
        }
        true
    }

    /// The session a node is the row of, or the workspace it is the
    /// header of.
    pub fn row_hit(&self, node: NodeId) -> Option<DrawerHit> {
        self.rows
            .iter()
            .find(|(id, _)| *id == node)
            .map(|(_, pane)| DrawerHit::Session(pane.clone()))
            .or_else(|| {
                self.headers
                    .iter()
                    .find(|(id, _)| *id == node)
                    .map(|(_, ws)| DrawerHit::Workspace(ws.clone()))
            })
    }

    /// The headers' workspace ids, in order.
    pub fn header_ids(&self) -> Vec<String> {
        self.headers.iter().map(|(_, w)| w.clone()).collect()
    }

    /// Where header `i` is now, in sheet coordinates (scrolled).
    pub fn header_rect(&self, router: &Router, i: usize) -> Option<Rect> {
        let (id, _) = self.headers.get(i)?;
        let h = router.get::<GroupHeader>(*id)?;
        let dy = self.scroll.offset()[1];
        Some(Rect::new(h.rect.x, h.rect.y - dy, h.rect.w, h.rect.h))
    }

    /// The rows' panes, in order.
    pub fn row_panes(&self) -> Vec<String> {
        self.rows.iter().map(|(_, p)| p.clone()).collect()
    }

    /// Where row `i` is now, in sheet coordinates (scrolled).
    pub fn row_rect(&self, router: &Router, i: usize) -> Option<Rect> {
        let (id, _) = self.rows.get(i)?;
        let row = router.get::<SessionRow>(*id)?;
        let dy = self.scroll.offset()[1];
        Some(Rect::new(
            row.rect.x,
            row.rect.y - dy,
            row.rect.w,
            row.rect.h,
        ))
    }

    /// The list's window, in sheet coordinates.
    pub fn list_rect(&self) -> Rect {
        self.list
    }

    /// The spinner's glyph cells of the rows that spin, in sheet
    /// coordinates, and their colours: drawn over the rows, in the
    /// terminal face, clipped to [`DrawerContent::list_rect`].
    pub fn spinners(&self, router: &Router) -> Vec<(Rect, [f32; 4])> {
        let s = self.scale;
        (0..self.rows.len())
            .filter_map(|i| {
                let (id, _) = &self.rows[i];
                let row = router.get::<SessionRow>(*id)?;
                (row.activity == Activity::Processing).then_some(())?;
                let r = self.row_rect(router, i)?;
                let col = Rect::new(r.x + r.w - COLUMN_W * s, r.y, COLUMN_W * s, r.h);
                (col.y + col.h > self.list.y && col.y < self.list.y + self.list.h)
                    .then_some((col, row.colour))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawer::remote_card::tests::info;
    use crate::drawer::session_row::tests::agent;
    use libflankr_proto::common::Turn;

    /// The S23 Ultra's width in points, at scale 1.
    const W: f32 = 384.0;

    fn remotes(n: usize) -> Vec<RemoteInfo> {
        (0..n)
            .map(|i| {
                let mut r = info(&format!("m{i}"));
                r.id = RemoteId([i as u8; 8]);
                r.active = i == 0;
                r.online = true;
                r
            })
            .collect()
    }

    #[test]
    fn the_grid_has_the_plus_in_the_slot_after_the_last_card_and_none_at_8() {
        // The sheet on a 384 pt screen: 0.85 of it.
        let sheet = W * 0.85;
        for (n, slots, rows) in [(0, 1, 1), (3, 4, 1), (4, 5, 2), (7, 8, 2), (8, 8, 2)] {
            let mut c = DrawerContent::default();
            c.set_remotes(remotes(n));
            c.set_frame([sheet, 800.0], [40.0, 20.0], 1.0);
            let mut router = Router::new();
            c.build(&mut router, Duration::ZERO);
            let s = c.slots();
            assert_eq!(s.len(), slots, "{n} remotes");
            assert_eq!(s.last() == Some(&Slot::Add), n < 8, "{n} remotes");
            assert_eq!(
                s.iter().filter(|x| **x == Slot::Add).count(),
                usize::from(n < 8)
            );
            let ys: HashSet<i32> = (0..slots)
                .map(|i| c.card_rect(i).unwrap().y as i32)
                .collect();
            assert_eq!(ys.len(), rows, "{n} remotes: rows");
            // 4 across, 8 pt apart, 12 pt in, below the top inset.
            let first = c.card_rect(0).unwrap();
            assert_eq!((first.x, first.y), (12.0, 52.0));
            assert!((first.w - (sheet - 24.0 - 24.0) / 4.0).abs() < 0.01);
            assert_eq!(first.h, 52.0);
            if slots > 4 {
                let fifth = c.card_rect(4).unwrap();
                assert_eq!((fifth.x, fifth.y), (12.0, 52.0 + 60.0));
            }
            // The list, 12 pt under the grid.
            let last = c.card_rect(slots - 1).unwrap();
            assert_eq!(c.list_rect().y, last.y + last.h + 12.0);
        }
    }

    #[test]
    fn a_tap_on_a_card_is_its_remote_and_on_the_plus_add() {
        let mut c = DrawerContent::default();
        c.set_remotes(remotes(2));
        c.set_frame([326.0, 800.0], [0.0, 0.0], 1.0);
        let mut router = Router::new();
        c.build(&mut router, Duration::ZERO);
        let at = |i| c.card_rect(i).unwrap().center();
        assert_eq!(c.card_at(at(1)), Some(1));
        assert_eq!(c.card_hit(1), Some(DrawerHit::Remote(RemoteId([1; 8]))));
        assert_eq!(c.card_hit(2), Some(DrawerHit::AddRemote));
        assert_eq!(c.card_at([2.0, 2.0]), None);
    }

    #[test]
    fn a_kept_list_shows_its_rows_while_offline_and_unload_shows_nothing() {
        crate::theme::testing::use_theme("dark_gray");
        let mut c = DrawerContent::default();
        let mut rs = remotes(1);
        rs[0].online = false;
        c.set_remotes(rs);
        c.set_frame([326.0, 800.0], [0.0, 0.0], 1.0);
        c.set_retry(Some(4));
        c.set_agents(vec![agent("p1", Some(1), None)], Duration::ZERO);
        let mut router = Router::new();
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["p1"]);
        assert_eq!(c.note(), None, "rows, not the retry");
        c.unload();
        c.build(&mut router, Duration::ZERO);
        assert!(c.row_panes().is_empty());
        assert_eq!(c.note(), Some(Note::Unreachable(4)));
    }

    #[test]
    fn the_rows_are_the_list_in_order_and_the_note_shows_without_one() {
        crate::theme::testing::use_theme("dark_gray");
        let mut c = DrawerContent::default();
        let mut rs = remotes(2);
        rs[0].online = false;
        c.set_remotes(rs.clone());
        c.set_frame([326.0, 800.0], [0.0, 0.0], 1.0);
        let mut router = Router::new();
        c.build(&mut router, Duration::ZERO);
        // Connecting: no looking-for message.
        assert_eq!(c.note(), None);
        c.set_grouped(false);
        c.set_retry(Some(4));
        assert_eq!(
            c.note_text(&c.note().unwrap()),
            "Can't reach m0 · retrying in 4 s"
        );
        rs[0].online = true;
        c.set_remotes(rs);
        c.set_retry(None);
        // Online with no list yet: nothing; an empty list: No sessions.
        assert_eq!(c.note(), None);
        c.set_agents(Vec::new(), Duration::ZERO);
        assert_eq!(c.note(), Some(Note::NoSessions));
        assert_eq!(c.note_text(&Note::NoSessions), "No sessions on m0");
        c.set_agents(
            vec![agent("p2", Some(2), None), agent("p1", Some(1), None)],
            Duration::ZERO,
        );
        c.set_open_pane(Some("p1".into()));
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.note(), None);
        assert_eq!(c.row_panes(), ["p2", "p1"], "agents.list order");
        let r0 = c.row_rect(&router, 0).unwrap();
        let r1 = c.row_rect(&router, 1).unwrap();
        assert_eq!(r1.y, r0.y + 44.0);
        assert_eq!(r0.y, c.list_rect().y);
        let open = |i: usize| router.get::<SessionRow>(c.rows[i].0).unwrap().open;
        assert!(!open(0) && open(1), "the view open under it");
        assert_eq!(
            c.row_hit(c.rows[1].0),
            Some(DrawerHit::Session("p1".into()))
        );
    }

    #[test]
    fn shell_panes_sit_below_the_agent_sessions_and_open_as_sessions() {
        // Plan 20261103_01, decision 7 and the 12:34 EST amendment, item 5.
        crate::theme::testing::use_theme("dark_gray");
        let mut c = DrawerContent::default();
        c.set_grouped(false);
        c.set_remotes(remotes(1));
        c.set_frame([326.0, 800.0], [0.0, 0.0], 1.0);
        let shell = |pane: &str| {
            let mut a = agent(pane, None, None);
            a.agent = "shell".into();
            a.cwd = Some(format!("/Users/me/src/{pane}-dir"));
            a
        };
        c.set_agents(
            vec![
                shell("s1"),
                agent("p2", Some(2), None),
                shell("s2"),
                agent("p1", Some(1), None),
            ],
            Duration::ZERO,
        );
        let mut router = Router::new();
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["p2", "p1", "s1", "s2"]);
        // A shell's row: its folder after its tab, and no spinner.
        let row = router.get::<SessionRow>(c.rows[2].0).unwrap();
        assert_eq!(row.summary, "s1-dir");
        assert!(c.spinners(&router).is_empty());
        // A tap on it opens it, as a session: the app shows it terminal
        // only.
        assert_eq!(
            c.row_hit(c.rows[2].0),
            Some(DrawerHit::Session("s1".into()))
        );
    }

    #[test]
    fn a_row_stops_spinning_recent_after_its_last_write_between_lists() {
        crate::theme::testing::use_theme("dark_gray");
        let mut c = DrawerContent::default();
        c.set_remotes(remotes(1));
        c.set_frame([326.0, 800.0], [0.0, 0.0], 1.0);
        let mut a = agent(
            "p1",
            Some(1),
            Some(Turn::Stopped {
                line: "done".into(),
            }),
        );
        a.idle_ms = Some(2_000);
        let t0 = Duration::from_secs(100);
        c.set_agents(vec![a], t0);
        let mut router = Router::new();
        c.build(&mut router, t0);
        assert_eq!(c.spinners(&router).len(), 1, "written 2 s ago");
        // The next frame of the spinner: drawn again.
        assert!(c.refresh(&mut router, t0 + Duration::from_millis(80)));
        assert!(!c.refresh(&mut router, t0 + Duration::from_millis(100)));
        // 10 s after the write: idle, "now".
        assert!(c.refresh(&mut router, t0 + Duration::from_secs(9)));
        assert!(c.spinners(&router).is_empty());
        let row = router.get::<SessionRow>(c.rows[0].0).unwrap();
        assert_eq!(
            row.indicator(),
            crate::drawer::session_row::Indicator::Age("now".into())
        );
        // Each spinning row shows the one clock's frame.
        assert!(!c.refresh(&mut router, t0 + Duration::from_secs(10)));
    }

    /// A session in workspace `ws`, number `n`, labelled `label`.
    fn in_ws(pane: &str, tab: u32, ws: &str, n: Option<u32>, label: Option<&str>) -> AgentInfo {
        let mut a = agent(pane, Some(tab), None);
        a.workspace_id = Some(ws.into());
        a.workspace_number = n;
        a.workspace_label = label.map(str::to_string);
        a.project = label.map(str::to_string);
        a.name = Some(match label {
            Some(l) => format!("Claude in {l}, tab {tab}-fl"),
            None => format!("Claude, tab {tab}-fl"),
        });
        a
    }

    fn shell_in(pane: &str, tab: u32, ws: &str, n: Option<u32>, label: Option<&str>) -> AgentInfo {
        let mut a = in_ws(pane, tab, ws, n, label);
        a.agent = "shell".into();
        a.turn = None;
        a
    }

    fn asking(mut a: AgentInfo) -> AgentInfo {
        a.turn = Some(Turn::Asking {
            question: "Go on?".into(),
        });
        a
    }

    fn grouped() -> (DrawerContent, Router) {
        crate::theme::testing::use_theme("dark_gray");
        let mut c = DrawerContent::default();
        assert!(c.grouped(), "grouped by default (decision 2)");
        c.set_remotes(remotes(1));
        c.set_frame([326.0, 2000.0], [0.0, 0.0], 1.0);
        (c, Router::new())
    }

    /// Each header's text and count, and the rows' panes, as built.
    fn shown(c: &DrawerContent, router: &Router) -> (Vec<(String, usize)>, Vec<String>) {
        let headers = c
            .headers
            .iter()
            .map(|(id, _)| {
                let h = router.get::<GroupHeader>(*id).unwrap();
                (h.text.clone(), h.count)
            })
            .collect();
        (headers, c.row_panes())
    }

    fn tap_header(c: &mut DrawerContent, ws: &str) {
        let node = c.headers.iter().find(|(_, w)| w == ws).unwrap().0;
        let Some(DrawerHit::Workspace(id)) = c.row_hit(node) else {
            panic!("a header's tap is its workspace");
        };
        c.toggle_group(&id);
    }

    #[test]
    fn headers_are_in_herdrs_order_with_their_sessions_in_tab_order_then_shells() {
        // Plan 20261103_02, decisions 3.1 and 3.2, and the 13:05 EST
        // amendment, item 3.
        let (mut c, mut router) = grouped();
        c.set_agents(
            vec![
                in_ws("a3", 3, "w1", Some(2), Some("libmorph")),
                shell_in("s1", 1, "w1", Some(2), Some("libmorph")),
                in_ws("b1", 1, "w2", Some(1), Some("flankr")),
                in_ws("a2", 2, "w1", Some(2), Some("libmorph")),
                in_ws("b5", 5, "w2", Some(1), Some("flankr")),
                // One session: no header, its full name.
                in_ws("l1", 1, "w3", Some(3), Some("libcontrols")),
                // No label, no project: by number; then no number, last.
                {
                    let mut a = in_ws("n1", 1, "w4", Some(4), None);
                    a.project = None;
                    a
                },
                in_ws("n2", 2, "w4", Some(4), None),
                in_ws("z1", 1, "w5", None, Some("zeta")),
                in_ws("z2", 2, "w5", None, Some("zeta")),
                in_ws("y1", 1, "w6", None, Some("alpha")),
                in_ws("y2", 2, "w6", None, Some("alpha")),
            ],
            Duration::ZERO,
        );
        c.build(&mut router, Duration::ZERO);
        let (headers, rows) = shown(&c, &router);
        assert_eq!(
            headers,
            [
                ("flankr".to_string(), 2),
                ("libmorph".to_string(), 3),
                ("Workspace 4".to_string(), 2),
                ("alpha".to_string(), 2),
                ("zeta".to_string(), 2),
            ]
        );
        assert_eq!(
            rows,
            [
                "b1", "b5", "a2", "a3", "s1", "l1", "n1", "n2", "y1", "y2", "z1", "z2"
            ]
        );
        // Under a header a row is its tab and summary, which leave out
        // the workspace the header shows; the one alone has its full name,
        // in the place its header would have had, after libmorph's.
        let row = |pane: &str| {
            let i = c.row_panes().iter().position(|p| p == pane).unwrap();
            router.get::<SessionRow>(c.rows[i].0).unwrap().clone()
        };
        assert_eq!(row("b1").tab, "1-fl");
        assert_eq!(row("l1").tab, "Claude in libcontrols, tab 1-fl");
        // Laid out in order: a 28 pt header, then 44 pt rows.
        let h0 = c.header_rect(&router, 0).unwrap();
        assert_eq!((h0.y, h0.h), (c.list_rect().y, 28.0));
        let r0 = c.row_rect(&router, 0).unwrap();
        assert_eq!(r0.y, h0.y + 28.0);
        assert_eq!(c.row_rect(&router, 1).unwrap().y, r0.y + 44.0);
        let h1 = c.header_rect(&router, 1).unwrap();
        assert_eq!(h1.y, r0.y + 88.0);
    }

    #[test]
    fn not_grouped_gives_todays_list() {
        // Decision 4: the list's order, shell panes at its end, no headers,
        // the rows' text as before.
        let (mut c, mut router) = grouped();
        c.set_grouped(false);
        c.set_agents(
            vec![
                in_ws("a3", 3, "w1", Some(2), Some("libmorph")),
                shell_in("s1", 1, "w1", Some(2), Some("libmorph")),
                in_ws("b1", 1, "w2", Some(1), Some("flankr")),
                in_ws("l1", 1, "w3", Some(3), Some("libcontrols")),
            ],
            Duration::ZERO,
        );
        c.build(&mut router, Duration::ZERO);
        let (headers, rows) = shown(&c, &router);
        assert!(headers.is_empty());
        assert_eq!(rows, ["a3", "b1", "l1", "s1"]);
        let tab = router.get::<SessionRow>(c.rows[2].0).unwrap().tab.clone();
        assert_eq!(tab, "1-fl", "no full name");
        // Grouped again: the headers come back.
        c.set_grouped(true);
        assert!(c.is_dirty());
        c.build(&mut router, Duration::ZERO);
        assert_eq!(shown(&c, &router).0.len(), 1, "libmorph's only");
    }

    #[test]
    fn a_tap_collapses_a_group_kept_per_remote_and_given_back_at_a_restart() {
        // Decision 3.4.
        let (mut c, mut router) = grouped();
        let list = vec![
            in_ws("a1", 1, "w1", Some(1), Some("libmorph")),
            in_ws("a2", 2, "w1", Some(1), Some("libmorph")),
            in_ws("b1", 1, "w2", Some(2), Some("flankr")),
            in_ws("b2", 2, "w2", Some(2), Some("flankr")),
        ];
        c.set_agents(list.clone(), Duration::ZERO);
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.take_collapsed(), None, "nothing changed");
        tap_header(&mut c, "w2");
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["a1", "a2"]);
        assert_eq!(c.header_ids(), ["w1", "w2"], "its header stays");
        let header = router.get::<GroupHeader>(c.headers[1].0).unwrap();
        assert!(header.collapsed && header.count == 2 && !header.dot());
        tap_header(&mut c, "w1");
        let kept = c.take_collapsed();
        assert_eq!(
            kept,
            Some((RemoteId([0; 8]), vec!["w1".to_string(), "w2".to_string()]))
        );
        assert_eq!(c.take_collapsed(), None, "taken once");
        // The client's echo of the first change does not undo the second.
        let mut rs = remotes(1);
        rs[0].collapsed_workspaces = vec!["w2".into()];
        c.set_remotes(rs);
        assert!(c.is_collapsed("w1") && c.is_collapsed("w2"));
        // Expanded again with a second tap.
        tap_header(&mut c, "w1");
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["a1", "a2"]);
        assert_eq!(c.take_collapsed().unwrap().1, ["w2"]);

        // A restart: the remote's kept groups, from its state.redb through
        // the client's remote rows.
        let (mut again, mut router) = grouped();
        let mut rs = remotes(2);
        rs[0].collapsed_workspaces = vec!["w2".into()];
        rs[1].collapsed_workspaces = vec!["w1".into()];
        again.set_remotes(rs.clone());
        again.set_agents(list, Duration::ZERO);
        again.build(&mut router, Duration::ZERO);
        assert_eq!(again.row_panes(), ["a1", "a2"]);
        assert_eq!(again.take_collapsed(), None, "nothing to keep");
        // The other remote in use: its own.
        rs[0].active = false;
        rs[1].active = true;
        again.set_remotes(rs);
        assert!(again.is_collapsed("w1") && !again.is_collapsed("w2"));
    }

    #[test]
    fn a_workspace_of_one_has_no_header_and_gets_it_back_collapsed_as_kept() {
        // The 13:05 EST amendment, item 3.
        let (mut c, mut router) = grouped();
        let mut rs = remotes(1);
        rs[0].collapsed_workspaces = vec!["w1".into()];
        c.set_remotes(rs);
        c.set_agents(
            vec![in_ws("a1", 1, "w1", Some(1), Some("libmorph"))],
            Duration::ZERO,
        );
        c.build(&mut router, Duration::ZERO);
        assert!(c.header_ids().is_empty());
        assert_eq!(c.row_panes(), ["a1"], "kept collapsed, but alone: shown");
        c.set_agents(
            vec![
                in_ws("a1", 1, "w1", Some(1), Some("libmorph")),
                in_ws("a2", 2, "w1", Some(1), Some("libmorph")),
            ],
            Duration::ZERO,
        );
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.header_ids(), ["w1"]);
        assert!(c.row_panes().is_empty(), "collapsed, as it was kept");
    }

    #[test]
    fn a_collapsed_header_shows_a_dot_while_a_session_in_it_waits() {
        // Decision 3.4: a question, or an approval, waiting.
        let (mut c, mut router) = grouped();
        let mut rs = remotes(1);
        rs[0].collapsed_workspaces = vec!["w1".into()];
        c.set_remotes(rs);
        let list = vec![
            asking(in_ws("a1", 1, "w1", Some(1), Some("libmorph"))),
            in_ws("a2", 2, "w1", Some(1), Some("libmorph")),
            in_ws("b1", 1, "w2", Some(2), Some("flankr")),
            in_ws("b2", 2, "w2", Some(2), Some("flankr")),
        ];
        // The first list: the group collapsed over a waiting session stays
        // collapsed, with its dot.
        c.set_agents(list.clone(), Duration::ZERO);
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["b1", "b2"]);
        let dot = |c: &DrawerContent, router: &Router, i: usize| {
            router.get::<GroupHeader>(c.headers[i].0).unwrap().dot()
        };
        assert!(dot(&c, &router, 0));
        assert!(!dot(&c, &router, 1), "expanded, no dot");
        // Answered: no dot.
        let mut answered = list.clone();
        answered[0].turn = None;
        c.set_agents(answered, Duration::ZERO);
        c.build(&mut router, Duration::ZERO);
        assert!(!dot(&c, &router, 0));
        // An approval waiting shows it too, and collapsing over it keeps
        // it collapsed.
        tap_header(&mut c, "w2");
        c.take_collapsed();
        c.set_pending(vec!["b2".into()]);
        c.build(&mut router, Duration::ZERO);
        assert!(c.row_panes().contains(&"b2".to_string()), "it expanded");
        tap_header(&mut c, "w2");
        c.build(&mut router, Duration::ZERO);
        assert!(dot(&c, &router, 1), "collapsed over the approval");
        assert!(c.row_panes().is_empty());
    }

    #[test]
    fn a_group_expands_when_a_session_in_it_starts_waiting_or_opens() {
        // Decision 3.5.
        let (mut c, mut router) = grouped();
        let list = vec![
            in_ws("a1", 1, "w1", Some(1), Some("libmorph")),
            in_ws("a2", 2, "w1", Some(1), Some("libmorph")),
            in_ws("b1", 1, "w2", Some(2), Some("flankr")),
            in_ws("b2", 2, "w2", Some(2), Some("flankr")),
        ];
        c.set_agents(list.clone(), Duration::ZERO);
        c.build(&mut router, Duration::ZERO);
        tap_header(&mut c, "w1");
        tap_header(&mut c, "w2");
        c.take_collapsed();
        c.build(&mut router, Duration::ZERO);
        assert!(c.row_panes().is_empty());
        // a2 starts asking: libmorph's group expands, and that is kept.
        let mut next = list.clone();
        next[1] = asking(next[1].clone());
        c.set_agents(next, Duration::ZERO);
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["a1", "a2"]);
        assert_eq!(c.take_collapsed().unwrap().1, ["w2"]);
        // The session opened in the agent view: its group expands.
        c.set_open_pane(Some("b1".into()));
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["a1", "a2", "b1", "b2"]);
        assert_eq!(c.take_collapsed().unwrap().1, Vec::<String>::new());
        // Collapsed after it opened: it stays collapsed.
        tap_header(&mut c, "w2");
        c.build(&mut router, Duration::ZERO);
        assert_eq!(c.row_panes(), ["a1", "a2"]);
    }
}
