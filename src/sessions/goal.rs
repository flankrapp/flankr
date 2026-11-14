//! A Claude Code session's goal (docs/design/20261112_03_ACTIVE_PLANS_HOME_VIEW_PLAN.md,
//! the 2026-11-13 11:58 EST amendment, item 1).
//!
//! Claude Code records a goal in the session's transcript: an
//! `attachment` line of type `goal_status` (`met`, `condition`), and the
//! text `Goal set: <goal>` as a queued command or a user message. The
//! newest such line, in file order, says what the goal is now: set to its
//! text, or ended (met, or cleared).

use std::fs::File;
use std::io::{BufRead as _, BufReader, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Lines longer than this are not goal lines (request blobs and the like
/// may quote a goal's text; they are never read as one).
const LINE_MAX: usize = 64 * 1024;

/// What one transcript line says of the goal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalLine {
    /// A goal set, with its text.
    Set(String),
    /// The goal met or cleared.
    Ended,
}

/// The text after `Goal set: `, or [`GoalLine::Ended`] for a text that
/// clears the goal.
fn from_text(text: &str) -> Option<GoalLine> {
    let text = text.trim();
    if let Some(goal) = text.strip_prefix("Goal set:") {
        let goal = goal.trim();
        return (!goal.is_empty()).then(|| GoalLine::Set(goal.to_string()));
    }
    (text.starts_with("Goal cleared") || text.starts_with("Goal removed"))
        .then_some(GoalLine::Ended)
}

/// What `line`, one transcript line, says of the goal, if anything.
pub fn goal_line(line: &[u8]) -> Option<GoalLine> {
    const NEEDLES: [&[u8]; 3] = [b"goal_status", b"Goal set:", b"Goal "];
    if line.len() > LINE_MAX
        || !NEEDLES
            .iter()
            .any(|n| line.windows(n.len()).any(|w| w == *n))
    {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    match v.get("type")?.as_str()? {
        "attachment" => {
            let a = v.get("attachment")?;
            match a.get("type")?.as_str()? {
                "goal_status" => {
                    let met = a.get("met").and_then(Value::as_bool).unwrap_or(false);
                    let condition = a
                        .get("condition")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or("");
                    Some(if met || condition.is_empty() {
                        GoalLine::Ended
                    } else {
                        GoalLine::Set(condition.to_string())
                    })
                }
                "queued_command" => from_text(a.get("prompt")?.as_str()?),
                _ => None,
            }
        }
        "queue-operation" if v.get("operation")?.as_str()? == "enqueue" => {
            from_text(v.get("content")?.as_str()?)
        }
        "user" if v.get("isSidechain").and_then(Value::as_bool) != Some(true) => {
            let content = v.get("message")?.get("content")?;
            match content {
                Value::String(s) => from_text(s),
                Value::Array(blocks) => blocks.iter().find_map(|b| {
                    (b.get("type")?.as_str()? == "text")
                        .then(|| from_text(b.get("text")?.as_str()?))
                        .flatten()
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// One file's goal, read from its start once and then from where it
/// stopped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoalScan {
    /// The offset just after the last whole line read.
    offset: u64,
    /// The newest goal line read, if any.
    last: Option<GoalLine>,
}

impl GoalScan {
    /// Read the whole lines `path` has past where this stopped. A file
    /// shorter than that is read again from its start.
    pub fn advance(&mut self, path: &Path) -> std::io::Result<()> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            *self = GoalScan::default();
        }
        if len == self.offset {
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut reader = BufReader::with_capacity(256 * 1024, file);
        let mut line = Vec::new();
        loop {
            line.clear();
            let got = reader.read_until(b'\n', &mut line)?;
            if got == 0 || line.last() != Some(&b'\n') {
                // The end, or a line still being written.
                break;
            }
            self.offset += got as u64;
            if let Some(g) = goal_line(&line[..line.len() - 1]) {
                self.last = Some(g);
            }
        }
        Ok(())
    }

    /// The newest goal line read, if any.
    pub fn last(&self) -> Option<&GoalLine> {
        self.last.as_ref()
    }
}

/// The goal of a session in `files` (oldest first), each read on from
/// its scan in `scans`: the newest goal line's, `None` when there is none
/// or it ended.
pub fn goal_of(
    files: &[PathBuf],
    scans: &mut std::collections::HashMap<PathBuf, GoalScan>,
) -> Option<String> {
    let mut newest = None;
    for path in files {
        let scan = scans.entry(path.clone()).or_default();
        if let Err(e) = scan.advance(path) {
            log::debug!("[goal] {}: {e}", path.display());
            continue;
        }
        if let Some(g) = scan.last() {
            newest = Some(g.clone());
        }
    }
    match newest? {
        GoalLine::Set(text) => Some(text),
        GoalLine::Ended => None,
    }
}

/// The most files a [`GoalCache`] keeps scans of before it starts again.
const CACHE_MAX: usize = 1024;

/// Goals for `agents.list` (docs/design/20261114_01_ACTIVE_TASKS_OVERVIEW_PLAN.md,
/// 2.1 item 2), never waited for: a session whose files have lines not
/// yet read is read on from where it stopped on a thread of its own, and
/// the goal known so far is given meanwhile (the first read of a long
/// transcript takes seconds).
#[derive(Debug, Default)]
pub struct GoalCache {
    inner: std::sync::Arc<std::sync::Mutex<CacheInner>>,
}

#[derive(Debug, Default)]
struct CacheInner {
    scans: std::collections::HashMap<PathBuf, GoalScan>,
    /// Sessions being read now, by their newest file.
    reading: std::collections::HashSet<PathBuf>,
}

impl GoalCache {
    /// The goal of the session in `files` (oldest first) as read so far;
    /// the files are read on in the background when they grew.
    pub fn current(&self, files: &[PathBuf]) -> Option<String> {
        let key = files.last()?.clone();
        let mut inner = self.inner.lock().unwrap();
        let behind = files.iter().any(|f| {
            let len = std::fs::metadata(f).map_or(0, |m| m.len());
            inner.scans.get(f).is_none_or(|s| s.offset != len)
        });
        if behind && inner.reading.insert(key.clone()) {
            if inner.scans.len() > CACHE_MAX {
                inner.scans.clear();
            }
            let mut scans: std::collections::HashMap<PathBuf, GoalScan> = files
                .iter()
                .filter_map(|f| Some((f.clone(), inner.scans.get(f)?.clone())))
                .collect();
            let files = files.to_vec();
            let shared = std::sync::Arc::clone(&self.inner);
            std::thread::spawn(move || {
                goal_of(&files, &mut scans);
                let mut inner = shared.lock().unwrap();
                inner.scans.extend(scans);
                inner.reading.remove(&key);
            });
        }
        known(files, &inner.scans)
    }

    /// Whether any session is being read now (tests).
    pub fn busy(&self) -> bool {
        !self.inner.lock().unwrap().reading.is_empty()
    }
}

/// The goal `scans` hold for `files`, read no further.
fn known(
    files: &[PathBuf],
    scans: &std::collections::HashMap<PathBuf, GoalScan>,
) -> Option<String> {
    let newest = files
        .iter()
        .filter_map(|f| scans.get(f)?.last())
        .next_back()?;
    match newest {
        GoalLine::Set(text) => Some(text.clone()),
        GoalLine::Ended => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    const SET: &str = r#"{"parentUuid":"a","isSidechain":false,"type":"attachment","attachment":{"type":"goal_status","met":false,"sentinel":true,"condition":"build the plan 20261101_01_WIDGET_PLAN"}}"#;
    const MET: &str = r#"{"parentUuid":"b","isSidechain":false,"type":"attachment","attachment":{"type":"goal_status","met":true,"sentinel":true,"condition":"build the plan 20261101_01_WIDGET_PLAN"}}"#;

    #[test]
    fn the_lines_that_say_what_the_goal_is() {
        assert_eq!(
            goal_line(SET.as_bytes()),
            Some(GoalLine::Set(
                "build the plan 20261101_01_WIDGET_PLAN".to_string()
            ))
        );
        assert_eq!(goal_line(MET.as_bytes()), Some(GoalLine::Ended));
        let queued = r#"{"type":"queue-operation","operation":"enqueue","content":"Goal set: finish docs/design/20261101_02_GADGET_PLAN.md"}"#;
        assert_eq!(
            goal_line(queued.as_bytes()),
            Some(GoalLine::Set(
                "finish docs/design/20261101_02_GADGET_PLAN.md".to_string()
            ))
        );
        let removed = r#"{"type":"queue-operation","operation":"remove","content":"Goal set: x"}"#;
        assert_eq!(goal_line(removed.as_bytes()), None, "only an enqueue");
        let attached = r#"{"type":"attachment","attachment":{"type":"queued_command","prompt":"Goal set: y"}}"#;
        assert_eq!(
            goal_line(attached.as_bytes()),
            Some(GoalLine::Set("y".to_string()))
        );
        let user = r#"{"type":"user","message":{"role":"user","content":"Goal cleared"}}"#;
        assert_eq!(goal_line(user.as_bytes()), Some(GoalLine::Ended));
        let blocks = r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Goal set: z"}]}}"#;
        assert_eq!(
            goal_line(blocks.as_bytes()),
            Some(GoalLine::Set("z".to_string()))
        );
        // Quoted by the agent, or in a request blob: not a goal.
        let said =
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Goal set: q"}]}}"#;
        assert_eq!(goal_line(said.as_bytes()), None);
        let blob = r#"{"type":"api-request-blob","message":{"content":"Goal set: q"}}"#;
        assert_eq!(goal_line(blob.as_bytes()), None);
        let side = r#"{"type":"user","isSidechain":true,"message":{"content":"Goal set: q"}}"#;
        assert_eq!(goal_line(side.as_bytes()), None, "a subagent's");
        assert_eq!(goal_line(b"{\"type\":\"user\"}"), None);
    }

    /// The active tasks overview plan, 2.1 item 2: the cache gives what
    /// it has read and reads the rest in the background.
    #[test]
    fn the_cache_reads_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        std::fs::write(&path, format!("{SET}\n")).unwrap();
        let cache = GoalCache::default();
        let files = vec![path.clone()];
        let settle = |cache: &GoalCache| {
            for _ in 0..200 {
                if !cache.busy() {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            panic!("never read");
        };
        let first = cache.current(&files);
        settle(&cache);
        let goal = Some("build the plan 20261101_01_WIDGET_PLAN".to_string());
        assert!(first.is_none() || first == goal, "what it had read");
        assert_eq!(cache.current(&files), goal);
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(f, "{MET}").unwrap();
        f.flush().unwrap();
        cache.current(&files);
        settle(&cache);
        assert_eq!(cache.current(&files), None, "met");
        assert_eq!(cache.current(&[]), None);
    }

    #[test]
    fn the_newest_goal_wins_and_is_read_on_from_where_it_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let mut f = File::create(&path).unwrap();
        writeln!(
            f,
            "{{\"type\":\"user\",\"message\":{{\"content\":\"hi\"}}}}"
        )
        .unwrap();
        writeln!(f, "{SET}").unwrap();
        f.flush().unwrap();
        let files = vec![path.clone()];
        let mut scans = std::collections::HashMap::new();
        assert_eq!(
            goal_of(&files, &mut scans).as_deref(),
            Some("build the plan 20261101_01_WIDGET_PLAN")
        );
        // A newer goal.
        writeln!(
            f,
            "{}",
            SET.replace("20261101_01_WIDGET_PLAN", "20261102_01_GADGET_PLAN")
        )
        .unwrap();
        // A line still being written is not read yet.
        write!(f, "{}", &MET[..20]).unwrap();
        f.flush().unwrap();
        assert_eq!(
            goal_of(&files, &mut scans).as_deref(),
            Some("build the plan 20261102_01_GADGET_PLAN")
        );
        // Finished: met, so no goal.
        writeln!(f, "{}", &MET[20..]).unwrap();
        f.flush().unwrap();
        assert_eq!(goal_of(&files, &mut scans), None);
        // A continued session: the newer file's goal line wins.
        let next = dir.path().join("t.jsonl");
        std::fs::write(&next, format!("{SET}\n")).unwrap();
        let both = vec![path.clone(), next];
        assert!(goal_of(&both, &mut scans).is_some());
        // A file rewritten shorter is read again from its start.
        std::fs::write(&path, format!("{SET}\n")).unwrap();
        assert!(goal_of(&files, &mut scans).is_some());
    }
}
