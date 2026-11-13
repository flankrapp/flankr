//! Worktrees (plan 20261112_02, 2.8 items 2 and 3): a pane in a worktree
//! of the project is judged against the main checkout's plans, and its
//! decisions name the worktree.

use std::path::{Path, PathBuf};

use super::super::PluginCtx;

/// The checkouts `git worktree list --porcelain` names, the main one
/// first.
pub fn porcelain_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(|p| PathBuf::from(p.trim()))
        .collect()
}

/// Where a context's project is checked out.
pub struct Checkout {
    /// A context on the main checkout, when the project is a worktree of
    /// it; `None` when the project is the main checkout, or git could not
    /// say.
    pub main: Option<PluginCtx>,
    /// The worktree's folder, when it is one.
    pub worktree: Option<String>,
}

/// The project's checkout: its declared `git worktree list` run in its
/// folder. Any failure reads as the main checkout.
pub async fn checkout(ctx: &PluginCtx) -> Checkout {
    let none = Checkout {
        main: None,
        worktree: None,
    };
    let Ok(project) = ctx.project() else {
        return none;
    };
    let out = match ctx.run_command("worktree_list", &[]).await {
        Ok(out) => out,
        Err(e) => {
            log::debug!("[policy] git worktree list: {}", e.message);
            return none;
        }
    };
    let paths = porcelain_paths(&out.stdout);
    let Some(main) = paths.first() else {
        return none;
    };
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    let main_root = canonical(main);
    if main_root.is_none() || main_root.as_deref() == Some(project.root.as_path()) {
        return none;
    }
    // Only a folder git names as one of the project's worktrees.
    if !paths
        .iter()
        .skip(1)
        .any(|p| canonical(p).as_deref() == Some(project.root.as_path()))
    {
        return none;
    }
    match ctx.rooted_at(main) {
        Ok(main_ctx) => Checkout {
            main: Some(main_ctx),
            worktree: Some(project.root.display().to_string()),
        },
        Err(e) => {
            log::info!(
                "[policy] the main checkout {}: {}",
                main.display(),
                e.message
            );
            none
        }
    }
}

/// The branch Work in a worktree makes for a plan: `plan/<YYYYMMDD_NN>`.
pub fn branch_for(path: &str) -> Option<String> {
    let base = path.rsplit('/').next()?.strip_suffix(".md")?;
    (base.len() >= 11 && base.as_bytes()[8] == b'_').then(|| format!("plan/{}", &base[..11]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_and_branches() {
        let text = "worktree /r\nHEAD abc\nbranch refs/heads/main\n\nworktree /r/.claude/worktrees/x\nHEAD def\nbranch refs/heads/plan/20261112_02\n";
        assert_eq!(
            porcelain_paths(text),
            [PathBuf::from("/r"), PathBuf::from("/r/.claude/worktrees/x")]
        );
        assert_eq!(
            branch_for("docs/design/20261112_02_PLAN_POLICY_AGENT_PLAN.md").as_deref(),
            Some("plan/20261112_02")
        );
        assert_eq!(branch_for("README"), None);
    }
}
