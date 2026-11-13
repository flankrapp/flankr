//! herdr's `worktree.*` API (vendor/herdr/src/api/schema/worktrees.rs):
//! requests built as JSON-RPC over [`HerdrRequest::Custom`], and the
//! `worktree.list` reply read into types. Every field herdr may leave out
//! is left out of the request.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::protocol::HerdrRequest;

/// `worktree.create`'s params; `None` fields are not sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeCreate {
    pub workspace_id: Option<String>,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    /// herdr starts the branch from `HEAD` without it.
    pub base: Option<String>,
    pub path: Option<String>,
    pub label: Option<String>,
    pub focus: bool,
}

/// `worktree.open`'s params; `None` fields are not sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeOpen {
    pub workspace_id: Option<String>,
    pub cwd: Option<String>,
    pub path: Option<String>,
    pub branch: Option<String>,
    pub label: Option<String>,
    pub focus: bool,
}

fn put(params: &mut Map<String, Value>, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        params.insert(key.to_string(), json!(v));
    }
}

fn request(method: &str, params: Map<String, Value>) -> HerdrRequest {
    HerdrRequest::Custom(json!({ "method": method, "params": Value::Object(params) }))
}

impl HerdrRequest {
    /// `worktree.list` for a workspace's repository, or the one at `cwd`.
    pub fn worktree_list(workspace_id: Option<&str>, cwd: Option<&str>) -> Self {
        let mut params = Map::new();
        put(
            &mut params,
            "workspace_id",
            &workspace_id.map(str::to_string),
        );
        put(&mut params, "cwd", &cwd.map(str::to_string));
        request("worktree.list", params)
    }

    /// `worktree.create`.
    pub fn worktree_create(p: &WorktreeCreate) -> Self {
        let mut params = Map::new();
        put(&mut params, "workspace_id", &p.workspace_id);
        put(&mut params, "cwd", &p.cwd);
        put(&mut params, "branch", &p.branch);
        put(&mut params, "base", &p.base);
        put(&mut params, "path", &p.path);
        put(&mut params, "label", &p.label);
        params.insert("focus".to_string(), json!(p.focus));
        request("worktree.create", params)
    }

    /// `worktree.open`.
    pub fn worktree_open(p: &WorktreeOpen) -> Self {
        let mut params = Map::new();
        put(&mut params, "workspace_id", &p.workspace_id);
        put(&mut params, "cwd", &p.cwd);
        put(&mut params, "path", &p.path);
        put(&mut params, "branch", &p.branch);
        put(&mut params, "label", &p.label);
        params.insert("focus".to_string(), json!(p.focus));
        request("worktree.open", params)
    }

    /// `worktree.remove` of the worktree open as `workspace_id`.
    pub fn worktree_remove(workspace_id: &str, force: bool) -> Self {
        let mut params = Map::new();
        params.insert("workspace_id".to_string(), json!(workspace_id));
        params.insert("force".to_string(), json!(force));
        request("worktree.remove", params)
    }
}

/// One worktree, as herdr lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeInfo {
    pub path: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub is_bare: bool,
    #[serde(default)]
    pub is_detached: bool,
    #[serde(default)]
    pub is_prunable: bool,
    #[serde(default)]
    pub is_linked_worktree: bool,
    #[serde(default)]
    pub open_workspace_id: Option<String>,
    #[serde(default)]
    pub label: String,
}

/// The repository a list is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSource {
    pub repo_key: String,
    pub repo_name: String,
    pub repo_root: String,
    pub source_checkout_path: String,
    #[serde(default)]
    pub source_workspace_id: Option<String>,
}

/// `worktree.list`'s reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeList {
    pub source: WorktreeSource,
    pub worktrees: Vec<WorktreeInfo>,
}

impl WorktreeList {
    /// The reply as the client returns it: the result, or a JSON-RPC
    /// envelope's `result`.
    pub fn from_reply(reply: &Value) -> Result<WorktreeList, serde_json::Error> {
        let result = reply.get("result").unwrap_or(reply);
        serde_json::from_value(result.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_requests_are_herdrs() {
        let rpc = HerdrRequest::worktree_list(Some("w1"), None).to_jsonrpc();
        assert_eq!(rpc.method, "worktree.list");
        assert_eq!(rpc.params, json!({"workspace_id": "w1"}));
        let rpc = HerdrRequest::worktree_create(&WorktreeCreate {
            workspace_id: Some("w1".into()),
            branch: Some("plan/20261112_02".into()),
            ..Default::default()
        })
        .to_jsonrpc();
        assert_eq!(rpc.method, "worktree.create");
        assert_eq!(
            rpc.params,
            json!({"workspace_id": "w1", "branch": "plan/20261112_02", "focus": false})
        );
        let rpc = HerdrRequest::worktree_open(&WorktreeOpen {
            cwd: Some("/r".into()),
            path: Some("/r/.claude/worktrees/x".into()),
            focus: true,
            ..Default::default()
        })
        .to_jsonrpc();
        assert_eq!(rpc.method, "worktree.open");
        assert_eq!(
            rpc.params,
            json!({"cwd": "/r", "path": "/r/.claude/worktrees/x", "focus": true})
        );
        let rpc = HerdrRequest::worktree_remove("w2", false).to_jsonrpc();
        assert_eq!(rpc.method, "worktree.remove");
        assert_eq!(rpc.params, json!({"workspace_id": "w2", "force": false}));
    }

    #[test]
    fn a_list_reply_from_a_fixture() {
        let text = include_str!("../tests/fixtures/worktree_list.json");
        let reply: Value = serde_json::from_str(text).unwrap();
        let list = WorktreeList::from_reply(&reply).unwrap();
        assert_eq!(list.source.repo_name, "flankr");
        assert_eq!(list.worktrees.len(), 2);
        assert!(!list.worktrees[0].is_linked_worktree);
        let linked = &list.worktrees[1];
        assert_eq!(linked.branch.as_deref(), Some("plan/20261112_02"));
        assert_eq!(linked.open_workspace_id.as_deref(), Some("w3"));
        assert_eq!(linked.label, "plan-policy");
        assert!(WorktreeList::from_reply(&json!({"type": "pong"})).is_err());
    }
}
