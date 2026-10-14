//! Git worktrees for delegated tasks (phase 4).
//!
//! A task that asks for `isolation: "worktree"` runs in its own `git worktree` on its own branch,
//! created from the repository's current commit under `<state_directory>/worktrees/<id>`. The
//! parent's folder is never touched. When the task is finished with, the worktree is removed if
//! it holds no uncommitted work; a branch with new commits is kept for the parent or the owner to
//! merge, an empty one is deleted. A worktree with uncommitted changes is kept and reported.
//!
//! Git runs with a literal argument list, no shell, no inherited `GIT_*` repository variables,
//! hooks pointed at an empty folder and `core.fsmonitor` off, so a repository's own
//! configuration cannot make the host run a program when a worktree is created or inspected.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;

/// Why a worktree could not be made, in words the delegating agent may see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unavailable {
    NotARepository,
    NoCommits,
    /// The repository's top folder is above the delegating task's folder: a worktree would give
    /// the child files its parent cannot see.
    RepositoryOutsideScope,
    GitFailed,
}
impl Unavailable {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotARepository => "the folder is not in a git repository",
            Self::NoCommits => "the repository has no commits yet",
            Self::RepositoryOutsideScope => {
                "the repository's top folder is outside the delegating task's folder"
            }
            Self::GitFailed => "git could not create the worktree",
        }
    }
}

/// What became of a worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Still in use by its task.
    Active,
    /// Being removed now.
    Releasing,
    /// Removed, and its branch deleted because it had no new commits.
    Removed,
    /// Removed; its branch holds new commits and was kept.
    BranchKept,
    /// Kept as it was, because it had uncommitted changes.
    KeptDirty,
    /// Cleanup failed; the worktree may still be on disk.
    CleanupFailed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Worktree {
    /// The worktree's top folder, under the host's state directory.
    pub path: PathBuf,
    /// The repository the worktree belongs to (its main working tree).
    pub repository: PathBuf,
    pub branch: String,
    /// The commit it started from.
    pub base: String,
    /// The task's folder inside the worktree, relative to its top (empty for the top).
    pub sub: PathBuf,
    /// The repository had uncommitted changes when the worktree was made; they are not in it.
    pub base_dirty: bool,
    pub outcome: Outcome,
}
impl Worktree {
    /// The folder the task runs in.
    pub fn task_folder(&self) -> PathBuf {
        if self.sub.as_os_str().is_empty() {
            self.path.clone()
        } else {
            self.path.join(&self.sub)
        }
    }
}

fn git(root: &Path, directory: &Path, args: &[&str]) -> Result<String> {
    let hooks = root.join(".no-hooks");
    std::fs::create_dir_all(&hooks).context("worktree folder")?;
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg(format!("core.hooksPath={}", hooks.display()))
        .args(["-c", "core.fsmonitor=false", "-c", "core.sshCommand=false"])
        .arg("-C")
        .arg(directory)
        .args(args)
        .stdin(std::process::Stdio::null());
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_COMMON_DIR",
        "GIT_CEILING_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
    ] {
        command.env_remove(name);
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().context("git could not be started")?;
    if !output.status.success() {
        bail!("git {} failed", args.first().copied().unwrap_or(""));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

fn canonical(path: &Path) -> PathBuf {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // Windows: `\\?\C:\x` and git's `C:/x` must compare equal.
    if cfg!(windows) {
        let text = path.to_string_lossy().replace('/', "\\");
        PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
    } else {
        path
    }
}

/// Make a worktree for a task running in `folder` (inside the delegating task's `parent_scope`).
pub fn create(root: &Path, folder: &Path, parent_scope: &Path) -> Result<Worktree, Unavailable> {
    let top = git(root, folder, &["rev-parse", "--show-toplevel"])
        .map_err(|_| Unavailable::NotARepository)?;
    let top = canonical(Path::new(&top));
    let folder = canonical(folder);
    if !top.starts_with(canonical(parent_scope)) {
        return Err(Unavailable::RepositoryOutsideScope);
    }
    let base = git(root, &top, &["rev-parse", "--verify", "HEAD^{commit}"])
        .map_err(|_| Unavailable::NoCommits)?;
    let base_dirty = git(
        root,
        &top,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .map(|out| !out.is_empty())
    .map_err(|_| Unavailable::GitFailed)?;
    let sub = folder
        .strip_prefix(&top)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let id = Uuid::new_v4();
    let path = root.join(id.to_string());
    let branch = format!("acpd/task-{}", &id.simple().to_string()[..12]);
    let target = path.to_string_lossy().into_owned();
    git(
        root,
        &top,
        &["worktree", "add", "--quiet", "-b", &branch, &target, &base],
    )
    .map_err(|_| Unavailable::GitFailed)?;
    let worktree = Worktree {
        path: canonical(&path),
        repository: top,
        branch,
        base,
        sub,
        base_dirty,
        outcome: Outcome::Active,
    };
    if !worktree.task_folder().is_dir() {
        // The folder is untracked (or empty) in the commit, so the worktree lacks it.
        let _ = std::fs::create_dir_all(worktree.task_folder());
    }
    Ok(worktree)
}

/// Finish with a worktree: remove it unless it has uncommitted changes, keep its branch only if
/// it holds new commits. Returns the outcome; an already finished worktree is left alone.
pub fn release(root: &Path, worktree: &Worktree) -> Outcome {
    if worktree.outcome != Outcome::Active {
        return worktree.outcome;
    }
    if !worktree.path.exists() {
        let _ = git(root, &worktree.repository, &["worktree", "prune"]);
        return branch_cleanup(root, worktree);
    }
    match git(
        root,
        &worktree.path,
        &["status", "--porcelain", "--untracked-files=normal"],
    ) {
        Ok(out) if !out.is_empty() => return Outcome::KeptDirty,
        Ok(_) => {}
        Err(_) => return Outcome::CleanupFailed,
    }
    let target = worktree.path.to_string_lossy().into_owned();
    if git(root, &worktree.repository, &["worktree", "remove", &target]).is_err() {
        return Outcome::CleanupFailed;
    }
    branch_cleanup(root, worktree)
}
fn branch_cleanup(root: &Path, worktree: &Worktree) -> Outcome {
    let range = format!("{}..{}", worktree.base, worktree.branch);
    match git(root, &worktree.repository, &["rev-list", "--count", &range]) {
        Ok(count) if count == "0" => {
            if git(
                root,
                &worktree.repository,
                &["branch", "-D", &worktree.branch],
            )
            .is_ok()
            {
                Outcome::Removed
            } else {
                Outcome::CleanupFailed
            }
        }
        Ok(_) => Outcome::BranchKept,
        // The branch is gone already (deleted by hand): nothing left to keep.
        Err(_) => Outcome::Removed,
    }
}

/// Folders under `root` that no record knows about (a host that stopped between creating a
/// worktree and saving its record). Each clean one is removed, its branch kept: without a
/// record the base is unknown, so new commits cannot be told apart. Returns how many were left.
pub fn sweep_unknown(root: &Path, known: &[PathBuf]) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let known: Vec<PathBuf> = known.iter().map(|p| canonical(p)).collect();
    let mut left = 0;
    for entry in entries.flatten() {
        let path = canonical(&entry.path());
        let name = entry.file_name();
        if name == ".no-hooks" || !path.is_dir() || known.contains(&path) {
            continue;
        }
        if Uuid::parse_str(&name.to_string_lossy()).is_err() {
            left += 1;
            continue;
        }
        let clean = git(root, &path, &["status", "--porcelain"]).is_ok_and(|out| out.is_empty());
        let common = git(
            root,
            &path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        );
        match (clean, common) {
            (true, Ok(common)) => {
                let repository = Path::new(&common).parent().map(Path::to_path_buf);
                let target = path.to_string_lossy().into_owned();
                let removed = repository.is_some_and(|repository| {
                    git(root, &repository, &["worktree", "remove", &target]).is_ok()
                });
                if !removed {
                    left += 1;
                }
            }
            _ => left += 1,
        }
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .output()
            .unwrap();
        assert!(status.status.success(), "{args:?}: {status:?}");
    }
    fn repo() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("src/lib.rs"), "one\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "-q", "-m", "first"]);
        let root = base.path().join("state/worktrees");
        (base, canonical(&repo), root)
    }

    #[test]
    fn a_worktree_is_its_own_checkout_on_its_own_branch_and_an_unused_one_disappears() {
        let (_base, repo, root) = repo();
        let wt = create(&root, &repo.join("src"), &repo).unwrap();
        assert!(wt.path.starts_with(canonical(&root)));
        assert!(wt.branch.starts_with("acpd/task-"));
        assert_eq!(wt.sub, PathBuf::from("src"));
        assert_eq!(
            std::fs::read_to_string(wt.task_folder().join("lib.rs")).unwrap(),
            "one\n"
        );
        assert!(!wt.base_dirty);
        assert_eq!(release(&root, &wt), Outcome::Removed);
        assert!(!wt.path.exists());
        let branches = git(&root, &repo, &["branch", "--list", "acpd/*"]).unwrap();
        assert!(branches.is_empty(), "{branches}");
    }

    #[test]
    fn committed_work_keeps_the_branch_and_uncommitted_work_keeps_the_worktree() {
        let (_base, repo, root) = repo();
        let committed = create(&root, &repo, &repo).unwrap();
        std::fs::write(committed.path.join("new.txt"), "x").unwrap();
        run(&committed.path, &["add", "."]);
        run(&committed.path, &["commit", "-q", "-m", "child"]);
        assert_eq!(release(&root, &committed), Outcome::BranchKept);
        assert!(!committed.path.exists());
        assert!(
            !git(&root, &repo, &["branch", "--list", &committed.branch])
                .unwrap()
                .is_empty()
        );

        let dirty = create(&root, &repo, &repo).unwrap();
        std::fs::write(dirty.path.join("src/lib.rs"), "changed\n").unwrap();
        assert_eq!(release(&root, &dirty), Outcome::KeptDirty);
        assert!(dirty.path.exists());
        // The parent's checkout never saw either child's work.
        assert_eq!(
            std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
            "one\n"
        );
        assert!(!repo.join("new.txt").exists());
    }

    #[test]
    fn non_repositories_empty_repositories_and_repositories_above_the_scope_are_refused() {
        let (base, repo, root) = repo();
        let plain = base.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(
            create(&root, &plain, &plain).unwrap_err(),
            Unavailable::NotARepository
        );
        let empty = base.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        run(&empty, &["init", "-q"]);
        assert_eq!(
            create(&root, &empty, &empty).unwrap_err(),
            Unavailable::NoCommits
        );
        assert_eq!(
            create(&root, &repo.join("src"), &repo.join("src")).unwrap_err(),
            Unavailable::RepositoryOutsideScope
        );
    }

    #[test]
    fn a_dirty_parent_is_reported_and_its_changes_stay_out_of_the_worktree() {
        let (_base, repo, root) = repo();
        std::fs::write(repo.join("src/lib.rs"), "parent edit\n").unwrap();
        let wt = create(&root, &repo, &repo).unwrap();
        assert!(wt.base_dirty);
        assert_eq!(
            std::fs::read_to_string(wt.path.join("src/lib.rs")).unwrap(),
            "one\n"
        );
        assert_eq!(release(&root, &wt), Outcome::Removed);
        assert_eq!(
            std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
            "parent edit\n"
        );
    }

    #[test]
    fn repository_hooks_do_not_run_when_a_worktree_is_made() {
        let (base, repo, root) = repo();
        let marker = base.path().join("hook-ran");
        let hook = repo.join(".git/hooks/post-checkout");
        std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
            // The hook works when git runs it normally, so the check below means something.
            run(&repo, &["checkout", "-q", "-b", "probe"]);
            assert!(marker.exists());
            std::fs::remove_file(&marker).unwrap();
            run(&repo, &["checkout", "-q", "main"]);
            std::fs::remove_file(&marker).unwrap();
        }
        let wt = create(&root, &repo, &repo).unwrap();
        assert!(!marker.exists());
        release(&root, &wt);
    }

    #[test]
    fn a_worktree_without_a_record_is_removed_when_clean_and_left_when_dirty() {
        let (_base, repo, root) = repo();
        let clean = create(&root, &repo, &repo).unwrap();
        let dirty = create(&root, &repo, &repo).unwrap();
        std::fs::write(dirty.path.join("scratch"), "x").unwrap();
        let kept = create(&root, &repo, &repo).unwrap();
        assert_eq!(sweep_unknown(&root, std::slice::from_ref(&kept.path)), 1);
        assert!(!clean.path.exists());
        assert!(dirty.path.exists() && kept.path.exists());
    }

    #[test]
    fn a_worktree_deleted_by_hand_is_pruned_and_its_empty_branch_removed() {
        let (_base, repo, root) = repo();
        let wt = create(&root, &repo, &repo).unwrap();
        std::fs::remove_dir_all(&wt.path).unwrap();
        assert_eq!(release(&root, &wt), Outcome::Removed);
        assert!(
            git(&root, &repo, &["worktree", "list"])
                .unwrap()
                .lines()
                .count()
                == 1
        );
    }
}
