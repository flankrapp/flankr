//! Worktrees (plan 20261112_02, 2.8 items 2 and 3): a pane in a worktree
//! of the project is judged against the main checkout's plans, and its
//! decisions name the worktree.

use std::path::{Path, PathBuf};

use super::super::PluginCtx;

/// The checkouts `git worktree list --porcelain` names, the main one
/// first.
pub fn porcelain_paths(text: &str) -> Vec<PathBuf> {
    porcelain(text).into_iter().map(|(p, _)| p).collect()
}

/// The checkouts with their branches (`refs/heads/` taken off).
pub fn porcelain(text: &str) -> Vec<(PathBuf, Option<String>)> {
    let mut out: Vec<(PathBuf, Option<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push((PathBuf::from(p.trim()), None));
        } else if let Some(b) = line.strip_prefix("branch ")
            && let Some(last) = out.last_mut()
        {
            let b = b.trim();
            last.1 = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        }
    }
    out
}

/// Where a context's project is checked out.
pub struct Checkout {
    /// A context on the main checkout, when the project is a worktree of
    /// it; `None` when the project is the main checkout, or git could not
    /// say.
    pub main: Option<PluginCtx>,
    /// The worktree's folder, when it is one.
    pub worktree: Option<String>,
    /// Its branch, when it has one.
    pub branch: Option<String>,
}

/// The project's checkout: its declared `git worktree list` run in its
/// folder. Any failure reads as the main checkout.
pub async fn checkout(ctx: &PluginCtx) -> Checkout {
    let none = Checkout {
        main: None,
        worktree: None,
        branch: None,
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
    let entries = porcelain(&out.stdout);
    let paths: Vec<PathBuf> = entries.iter().map(|(p, _)| p.clone()).collect();
    let Some(main) = paths.first() else {
        return none;
    };
    let canonical = |p: &Path| std::fs::canonicalize(p).ok();
    let main_root = canonical(main);
    if main_root.is_none() || main_root.as_deref() == Some(project.root.as_path()) {
        return none;
    }
    // Only a folder git names as one of the project's worktrees.
    let Some((_, branch)) = entries
        .iter()
        .skip(1)
        .find(|(p, _)| canonical(p).as_deref() == Some(project.root.as_path()))
    else {
        return none;
    };
    match ctx.rooted_at(main) {
        Ok(main_ctx) => Checkout {
            main: Some(main_ctx),
            worktree: Some(project.root.display().to_string()),
            branch: branch.clone(),
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
        assert_eq!(porcelain(text)[1].1.as_deref(), Some("plan/20261112_02"));
    }
}
