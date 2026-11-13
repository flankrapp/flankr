//! Each agent's herdr worktree for `agents.list` (plan 20261112_03, the
//! 21:58 EST amendment, item 4): herdr's `worktree.list` per workspace,
//! on the local herdr only, kept for [`TTL`]. A herdr without the call,
//! or one that fails, gives none.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use libflankr_proto::common::WorktreeInfo;
use libherdr::client::HerdrClient;
use libherdr::transport::HerdrTransport;

/// How long one workspace's list is kept.
pub const TTL: Duration = Duration::from_secs(15);
/// How long one `worktree.list` may take.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// The lists, by workspace id.
#[derive(Default)]
pub struct Worktrees {
    lists: Mutex<HashMap<String, (Instant, Vec<WorktreeInfo>)>>,
}

impl Worktrees {
    /// The linked worktrees of each of `workspaces`, asked again past
    /// [`TTL`]; nothing on a remote herdr.
    pub async fn of(
        &self,
        client: &HerdrClient,
        workspaces: impl IntoIterator<Item = String>,
    ) -> HashMap<String, Vec<WorktreeInfo>> {
        if !matches!(client.transport(), HerdrTransport::LocalSocket { .. }) {
            return HashMap::new();
        }
        let mut out = HashMap::new();
        for ws in workspaces {
            let kept = {
                let lists = self.lists.lock().unwrap();
                lists
                    .get(&ws)
                    .filter(|(at, _)| at.elapsed() < TTL)
                    .map(|(_, l)| l.clone())
            };
            let list = match kept {
                Some(list) => list,
                None => {
                    let list = ask(client, &ws).await;
                    self.lists
                        .lock()
                        .unwrap()
                        .insert(ws.clone(), (Instant::now(), list.clone()));
                    list
                }
            };
            out.insert(ws, list);
        }
        out
    }
}

async fn ask(client: &HerdrClient, workspace_id: &str) -> Vec<WorktreeInfo> {
    let reply =
        match tokio::time::timeout(TIMEOUT, client.worktree_list(Some(workspace_id), None)).await {
            Ok(Ok(reply)) => reply,
            Ok(Err(e)) => {
                log::debug!("worktree.list {workspace_id}: {e}");
                return Vec::new();
            }
            Err(_) => return Vec::new(),
        };
    match libherdr::WorktreeList::from_reply(&reply) {
        Ok(list) => list
            .worktrees
            .into_iter()
            .filter(|w| w.is_linked_worktree)
            .map(|w| WorktreeInfo {
                path: w.path,
                branch: w.branch,
                label: (!w.label.is_empty()).then_some(w.label),
                is_detached: w.is_detached,
                is_prunable: w.is_prunable,
                open_workspace_id: w.open_workspace_id,
            })
            .collect(),
        Err(e) => {
            log::debug!("worktree.list {workspace_id}: {e}");
            Vec::new()
        }
    }
}

/// The worktree among `list` that `cwd` lies in: its path, or a folder
/// under it; the deepest when several hold.
pub fn worktree_of(list: &[WorktreeInfo], cwd: &str) -> Option<WorktreeInfo> {
    let cwd = cwd.trim_end_matches('/');
    list.iter()
        .filter(|w| {
            let p = w.path.trim_end_matches('/');
            cwd == p
                || cwd
                    .strip_prefix(p)
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|w| w.path.len())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cwd_in_a_worktree() {
        let wt = |p: &str| WorktreeInfo {
            path: p.into(),
            ..WorktreeInfo::default()
        };
        let list = [wt("/r/.claude/worktrees/a"), wt("/r/.claude/worktrees/a/b")];
        assert_eq!(
            worktree_of(&list, "/r/.claude/worktrees/a").unwrap().path,
            "/r/.claude/worktrees/a"
        );
        assert_eq!(
            worktree_of(&list, "/r/.claude/worktrees/a/src")
                .unwrap()
                .path,
            "/r/.claude/worktrees/a"
        );
        assert_eq!(
            worktree_of(&list, "/r/.claude/worktrees/a/b/c")
                .unwrap()
                .path,
            "/r/.claude/worktrees/a/b"
        );
        assert_eq!(worktree_of(&list, "/r/.claude/worktrees/ab"), None);
        assert_eq!(worktree_of(&list, "/r"), None);
    }

    #[tokio::test]
    async fn nothing_from_a_herdr_that_is_not_local() {
        let w = Worktrees::default();
        assert!(
            w.of(&HerdrClient::mock(), ["w1".to_string()])
                .await
                .is_empty()
        );
    }
}
