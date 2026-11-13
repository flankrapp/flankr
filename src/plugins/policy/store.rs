//! The active tasks, kept by the daemon (plan 20261112_02, 2.3 item 1 as
//! the 22:31 EST amendment, item 2, has it):
//! `~/.local/state/flankr/plugins/policy/approvals.toml`, mode 0600, one
//! `[[approval]]` per task. A plan is armed while it has a task.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One active task: a plan the owner chose Build on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// The project's root, canonical: the main checkout.
    pub project_root: String,
    /// The plan, relative to the root: `docs/design/<base>.md`.
    pub path: String,
    /// The pane Build was chosen for, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    /// The plan as it was when armed, or when autocontinue was last
    /// switched on.
    pub sha256: String,
    #[serde(default = "yes")]
    pub armed: bool,
    #[serde(default)]
    pub autocontinue: bool,
    /// Who armed it: a principal id.
    pub by: String,
    /// When it was armed, RFC 3339.
    pub at: String,
    /// When autocontinue was last switched on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default)]
    pub continues_used: u32,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    approval: Vec<Task>,
}

/// The store at one path.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn at(path: impl Into<PathBuf>) -> Store {
        Store { path: path.into() }
    }

    /// The file under the daemon's state folder.
    pub fn under(state_dir: &Path) -> Store {
        Store::at(state_dir.join("plugins/policy/approvals.toml"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every task; none when the file is missing. A file that does not
    /// parse is logged and read as none, and is not written over until a
    /// change is made.
    pub fn load(&self) -> Vec<Task> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => match toml::from_str::<File>(&text) {
                Ok(file) => file.approval,
                Err(e) => {
                    log::warn!("[policy] {} does not parse: {e}", self.path.display());
                    Vec::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                log::warn!("[policy] {}: {e}", self.path.display());
                Vec::new()
            }
        }
    }

    /// Writes `tasks`, with the folder 0700 and the file 0600.
    pub fn save(&self, tasks: &[Task]) -> Result<(), String> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let dir = self.path.parent().ok_or("the store has no folder")?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let file = File {
            approval: tasks.to_vec(),
        };
        let text = format!(
            "# The plan policy plugin's active tasks, written by flankr.\n{}",
            toml::to_string(&file).map_err(|e| e.to_string())?
        );
        let tmp = self.path.with_extension("toml.tmp");
        let _ = std::fs::remove_file(&tmp);
        let written = (|| {
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            out.write_all(text.as_bytes())?;
            out.sync_all()?;
            std::fs::rename(&tmp, &self.path)
        })();
        written.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("{}: {e}", self.path.display())
        })
    }

    /// The task of `path` in `root`.
    pub fn find(&self, root: &str, path: &str) -> Option<Task> {
        self.load()
            .into_iter()
            .find(|t| t.project_root == root && t.path == path)
    }

    /// Changes the task of `path` in `root` with `change` (`None` removes
    /// it, a new value puts it), and saves; the task as it now is.
    pub fn update(
        &self,
        root: &str,
        path: &str,
        change: impl FnOnce(Option<Task>) -> Option<Task>,
    ) -> Result<Option<Task>, String> {
        let mut tasks = self.load();
        let at = tasks
            .iter()
            .position(|t| t.project_root == root && t.path == path);
        let old = at.map(|i| tasks.remove(i));
        let new = change(old);
        if let Some(task) = &new {
            tasks.push(task.clone());
        }
        self.save(&tasks)?;
        Ok(new)
    }
}

/// The SHA-256 of a plan's text, and its full length, in hex: any edit,
/// an amendment included, changes it.
pub fn plan_hash(text: &str, len: u64) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(len.to_le_bytes());
    h.update(text.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(path: &str) -> Task {
        Task {
            project_root: "/p".into(),
            path: path.into(),
            pane_id: Some("w1:p1".into()),
            sha256: plan_hash("x", 1),
            armed: true,
            autocontinue: false,
            by: "paired:phone".into(),
            at: "2026-11-12T22:00:00-05:00".into(),
            started_at: None,
            continues_used: 0,
        }
    }

    #[test]
    fn tasks_are_kept_private_and_read_back() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::under(dir.path());
        assert!(store.load().is_empty());
        let a = task("docs/design/20261112_02_A_PLAN.md");
        store.update("/p", &a.path, |_| Some(a.clone())).unwrap();
        assert_eq!(store.load(), std::slice::from_ref(&a));
        let mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let folder = std::fs::metadata(store.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(folder & 0o777, 0o700);
        assert_eq!(store.find("/p", &a.path), Some(a.clone()));
        assert_eq!(store.find("/q", &a.path), None);
        store.update("/p", &a.path, |_| None).unwrap();
        assert!(store.load().is_empty());
        std::fs::write(store.path(), "not toml [[").unwrap();
        assert!(store.load().is_empty());
    }

    #[test]
    fn the_hash_changes_with_any_edit() {
        assert_eq!(plan_hash("a", 1), plan_hash("a", 1));
        assert_ne!(plan_hash("a", 1), plan_hash("a\n", 2));
        assert_ne!(plan_hash("a", 1), plan_hash("a", 70_000), "a cut read");
        assert_eq!(plan_hash("", 0).len(), 64);
    }
}
