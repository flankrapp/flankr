//! Whether an agent is still producing
//! (docs/design/20261114_01_ACTIVE_TASKS_OVERVIEW_PLAN.md, 2.2): herdr says
//! `working` for as long as the agent's process is busy, so the daemon
//! looks further. While herdr says a pane is working, the pane's last
//! lines are read for the elapsed timer the agent draws (Claude Code's
//! `(1m 23s · …`); the pane is working while that timer is later than it
//! was on the previous read. With no timer, it is working while its
//! transcript has a tool call with no result yet, or was written within
//! [`SUBAGENT_LIVE`].

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use super::{EntryKind, Item, SUBAGENT_LIVE};

/// How many of the pane's visible lines are read for its timer (2.2
/// item 1).
pub const TIMER_LINES: usize = 12;

/// The least time between two reads of one pane's timer: reads asked
/// sooner (two phones listing at once) take the last read's verdict.
pub const READ_EVERY: Duration = Duration::from_secs(2);

/// A previous read older than this is not compared with: the timer it
/// saw may be another turn's.
pub const COMPARE_WITHIN: Duration = Duration::from_secs(60);

/// The elapsed timer in `text`, in seconds: the last run matching
/// `\((?:\d+h )?(?:\d+m )?\d+s\b`, such as `(12s ·`, `(1m 23s ·` or
/// `(1h 2m 3s ·`. `None` when there is none.
pub fn timer_secs(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    let mut found = None;
    for (i, _) in text.match_indices('(') {
        if let Some(secs) = timer_at(&b[i + 1..]) {
            found = Some(secs);
        }
    }
    found
}

/// The timer that starts `b` (just after its `(`), if one does.
fn timer_at(b: &[u8]) -> Option<u64> {
    let mut at = 0;
    let mut total: u64 = 0;
    // Up to three parts, `h` then `m` then `s`, each digits and a unit,
    // the `h` and `m` parts followed by one space.
    let mut units: &[u8] = b"hms";
    loop {
        let start = at;
        while at < b.len() && b[at].is_ascii_digit() {
            at += 1;
        }
        if at == start || at >= b.len() {
            return None;
        }
        let n: u64 = std::str::from_utf8(&b[start..at]).ok()?.parse().ok()?;
        let unit = b[at];
        let pos = units.iter().position(|u| *u == unit)?;
        units = &units[pos + 1..];
        at += 1;
        match unit {
            b'h' => total += n * 3600,
            b'm' => total += n * 60,
            _ => {
                // `\b`: the `s` ends a word.
                let ends = b
                    .get(at)
                    .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
                return ends.then_some(total + n);
            }
        }
        if b.get(at) != Some(&b' ') {
            return None;
        }
        at += 1;
    }
}

/// Whether `items` (a transcript's newest, oldest first) end with a tool
/// call that has no result after it.
pub fn open_tool_call(items: &[Item]) -> bool {
    let Some(at) = items.iter().rposition(|i| i.kind == EntryKind::ToolUse) else {
        return false;
    };
    let Some(id) = items[at].call_id.as_deref() else {
        return false;
    };
    !items[at + 1..]
        .iter()
        .any(|i| i.kind == EntryKind::ToolResult && i.call_id.as_deref() == Some(id))
}

/// Whether a transcript read at `now` is working by the rule without a
/// timer (2.2 items 2 and 3): a tool call open, or written within
/// [`SUBAGENT_LIVE`].
pub fn transcript_working(open_call: bool, written: Option<SystemTime>, now: SystemTime) -> bool {
    open_call || written.is_some_and(|t| now.duration_since(t).unwrap_or_default() <= SUBAGENT_LIVE)
}

/// What a pane's timer says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timer {
    /// Later than on the previous read: working.
    Advancing,
    /// Shown, and not later: not working.
    Stalled,
    /// None shown, or no previous read to compare with: the transcript
    /// decides.
    Unknown,
}

/// One pane's timer as last read.
#[derive(Debug, Clone, Copy)]
struct TimerState {
    secs: Option<u64>,
    read_at: SystemTime,
    timer: Timer,
    advanced_at: Option<SystemTime>,
}

/// The panes' timers, as read for `agents.list`.
#[derive(Debug, Default)]
pub struct PaneTimers {
    panes: Mutex<HashMap<String, TimerState>>,
}

impl PaneTimers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `pane`'s timer is due to be read at `now`: not read within
    /// [`READ_EVERY`].
    pub fn due(&self, pane: &str, now: SystemTime) -> bool {
        self.panes
            .lock()
            .unwrap()
            .get(pane)
            .is_none_or(|s| now.duration_since(s.read_at).unwrap_or_default() >= READ_EVERY)
    }

    /// Records a read of `pane` at `now` that showed `secs` (`None`: no
    /// timer), and gives its verdict.
    pub fn observe(&self, pane: &str, secs: Option<u64>, now: SystemTime) -> Timer {
        let mut panes = self.panes.lock().unwrap();
        let prev = panes.get(pane).copied();
        let comparable = prev
            .filter(|p| now.duration_since(p.read_at).unwrap_or_default() <= COMPARE_WITHIN)
            .and_then(|p| p.secs);
        let timer = match (comparable, secs) {
            (Some(p), Some(v)) if v > p => Timer::Advancing,
            (Some(_), Some(_)) => Timer::Stalled,
            _ => Timer::Unknown,
        };
        let advanced_at = if timer == Timer::Advancing {
            Some(now)
        } else {
            prev.and_then(|p| p.advanced_at)
        };
        panes.insert(
            pane.to_string(),
            TimerState {
                secs,
                read_at: now,
                timer,
                advanced_at,
            },
        );
        timer
    }

    /// `pane`'s verdict as last read, and when its timer last advanced.
    pub fn last(&self, pane: &str) -> (Timer, Option<SystemTime>) {
        self.panes
            .lock()
            .unwrap()
            .get(pane)
            .map_or((Timer::Unknown, None), |s| (s.timer, s.advanced_at))
    }

    /// When `pane`'s timer last advanced, kept while herdr says it is not
    /// working; its verdict goes, so the next read compares afresh.
    pub fn idle(&self, pane: &str) -> Option<SystemTime> {
        let mut panes = self.panes.lock().unwrap();
        let s = panes.get_mut(pane)?;
        s.secs = None;
        s.timer = Timer::Unknown;
        s.advanced_at
    }

    /// Forgets every pane not in `live`.
    pub fn retain(&self, live: &HashSet<String>) {
        self.panes.lock().unwrap().retain(|p, _| live.contains(p));
    }
}

/// Whether a pane is working (2.2 items 1 and 2): herdr says `working`,
/// and its timer advanced, or with no verdict from the timer the
/// transcript rule holds.
pub fn pane_working(
    herdr_working: bool,
    timer: Timer,
    open_call: bool,
    written: Option<SystemTime>,
    now: SystemTime,
) -> bool {
    herdr_working
        && match timer {
            Timer::Advancing => true,
            Timer::Stalled => false,
            Timer::Unknown => transcript_working(open_call, written, now),
        }
}

/// `t` as RFC 3339 in UTC, to the second.
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    super::agy::format_rfc3339(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::Role;

    #[test]
    fn the_timer_claude_code_draws() {
        assert_eq!(timer_secs("✻ Thinking… (12s · ↓ 1.2k tokens)"), Some(12));
        assert_eq!(timer_secs("(1m 23s · esc to interrupt)"), Some(83));
        assert_eq!(timer_secs("(1h 2m 3s ·"), Some(3723));
        assert_eq!(timer_secs("(1h 5s ·"), Some(3605));
        // The last on the screen is the one that counts.
        assert_eq!(timer_secs("(3s) old\n✻ Working (1m 4s · x)"), Some(64));
        // Not a timer.
        for text in [
            "",
            "plain (text)",
            "(12 s ·",
            "(12sx ·",
            "(m 3s",
            "(2m3s",
            "(5h)",
            "✻ Worked for 1m 23s",
        ] {
            assert_eq!(timer_secs(text), None, "{text:?}");
        }
        assert_eq!(timer_secs("(7s)"), Some(7));
    }

    /// Step 2's **Done when:** a pane showing `(1m 2s ·` then `(1m 4s ·`
    /// is working, and one showing `(1m 4s ·` twice is not.
    #[test]
    fn a_pane_is_working_while_its_timer_advances() {
        let timers = PaneTimers::new();
        let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let read = |s: &str| timer_secs(s);
        let first = timers.observe("p1", read("✻ Working (1m 2s · ↓ 2k tokens)"), t0);
        assert_eq!(first, Timer::Unknown, "nothing to compare with yet");
        assert!(!timers.due("p1", t0 + Duration::from_secs(1)));
        let t1 = t0 + Duration::from_secs(2);
        assert!(timers.due("p1", t1));
        let second = timers.observe("p1", read("✻ Working (1m 4s · ↓ 2k tokens)"), t1);
        assert_eq!(second, Timer::Advancing);
        assert!(pane_working(true, second, false, None, t1));
        assert_eq!(timers.last("p1"), (Timer::Advancing, Some(t1)));
        // The same timer twice: stalled, not working, whatever the
        // transcript says.
        let t2 = t1 + Duration::from_secs(2);
        let third = timers.observe("p1", read("✻ Working (1m 4s · ↓ 2k tokens)"), t2);
        assert_eq!(third, Timer::Stalled);
        assert!(!pane_working(true, third, true, Some(t2), t2));
        assert_eq!(timers.last("p1").1, Some(t1), "when it last advanced");
        // herdr not working: never working.
        assert!(!pane_working(false, Timer::Advancing, true, Some(t2), t2));
        // A read long after is not compared with.
        let later = t2 + COMPARE_WITHIN + Duration::from_secs(1);
        assert_eq!(
            timers.observe("p1", read("(1m 9s ·"), later),
            Timer::Unknown
        );
        timers.retain(&HashSet::new());
        assert_eq!(timers.last("p1"), (Timer::Unknown, None));
    }

    fn item(kind: EntryKind, id: &str) -> Item {
        Item {
            pos: 0,
            ts: "t".to_string(),
            role: if kind == EntryKind::ToolUse {
                Role::Assistant
            } else {
                Role::Tool
            },
            kind,
            tool: Some("Bash".to_string()),
            text: "Bash {}".to_string(),
            call_id: Some(id.to_string()),
            origin: None,
            choices: None,
            choice_result: None,
        }
    }

    /// A pane with no timer and an open tool call is working.
    #[test]
    fn without_a_timer_an_open_tool_call_or_a_recent_write() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(2_000_000);
        let open = vec![
            item(EntryKind::ToolResult, "c0"),
            item(EntryKind::ToolUse, "c1"),
        ];
        assert!(open_tool_call(&open));
        let mut closed = open.clone();
        closed.push(item(EntryKind::ToolResult, "c1"));
        assert!(!open_tool_call(&closed));
        assert!(!open_tool_call(&[]));
        let old = Some(now - Duration::from_secs(600));
        assert!(pane_working(true, Timer::Unknown, true, old, now));
        assert!(!pane_working(true, Timer::Unknown, false, old, now));
        assert!(pane_working(
            true,
            Timer::Unknown,
            false,
            Some(now - Duration::from_secs(30)),
            now
        ));
        assert_eq!(
            rfc3339(SystemTime::UNIX_EPOCH + Duration::from_secs(951_868_800)),
            "2000-03-01T00:00:00Z"
        );
    }
}
