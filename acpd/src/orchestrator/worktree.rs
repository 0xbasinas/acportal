//! Git worktrees for delegated tasks (phase 4).
//!
//! A task that asks for `isolation: "worktree"` runs in its own `git worktree` on its own branch,
//! created from the repository's current commit under `<state_directory>/worktrees/<id>`. The
//! parent's folder is never touched. When the task is finished with, the worktree is removed if
//! it holds no uncommitted work; a branch with new commits is kept for the parent or the owner to
//! merge, an empty one is deleted. A worktree with uncommitted changes is kept and reported.
//!
//! Git runs with a literal argument list, no shell, no inherited `GIT_*` repository variables,
//! hooks pointed at an empty folder and `core.fsmonitor` and `core.sshCommand` off, so a
//! repository's hooks and those two settings cannot make the host run a program.
//!
//! What this does not stop: programs the repository's own configuration names for content
//! filters (`filter.*.clean`/`smudge`, e.g. Git LFS), merge drivers (`merge.*.driver`) and diff
//! text converters. They run when a worktree is checked out, snapshotted or merged, as the host
//! user. That is the operator's own configuration, and a delegated agent that could change it
//! (by writing to `.git/config` with its own tools) can already run programs as that user, so
//! this is not a new reach, but it is not blocked either.
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
    /// Its uncommitted changes were committed to its branch (a snapshot), then it was removed;
    /// the branch was kept.
    Snapshotted,
    /// Kept as it was, because it had uncommitted changes (and they were not snapshotted).
    KeptDirty,
    /// Cleanup failed; the worktree may still be on disk.
    CleanupFailed,
    /// The owner merged its branch into the parent's checkout from the phone; the branch is gone.
    Merged,
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
    /// The merge commit, once the owner merged the branch into the parent's checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged: Option<String>,
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
        // The snapshot commit is the host's own, whoever runs the host.
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_AUTHOR_DATE",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "GIT_COMMITTER_DATE",
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

/// Why a merge into the parent's checkout was refused. Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeRefusal {
    /// The branch is gone (deleted by hand, or already merged).
    NoBranch,
    /// The branch has nothing the parent's checkout lacks.
    NothingToMerge,
    /// The parent's checkout has uncommitted changes (or untracked files) a merge could clobber.
    ParentDirty,
    /// The branch conflicts with the parent's checkout; the merge was aborted. Conflicting files.
    Conflict(Vec<String>),
    GitFailed,
}
impl MergeRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoBranch => "branch_missing",
            Self::NothingToMerge => "nothing_to_merge",
            Self::ParentDirty => "parent_dirty",
            Self::Conflict(_) => "merge_conflict",
            Self::GitFailed => "git_failed",
        }
    }
}

/// Merge a released worktree's branch into the checkout it was made from (the parent's folder,
/// on whatever branch it has checked out), as one `--no-ff` merge commit by `acpd`, without
/// hooks or signing. Refused, with nothing changed, when that checkout is dirty, when there is
/// nothing to merge, or when the branch conflicts (the merge is aborted). The branch is deleted
/// after a merge. Returns the merge commit.
pub fn merge(root: &Path, worktree: &Worktree) -> Result<String, MergeRefusal> {
    let repo = &worktree.repository;
    let reference = format!("refs/heads/{}", worktree.branch);
    if git(
        root,
        repo,
        &["rev-parse", "--verify", "--quiet", &reference],
    )
    .is_err()
    {
        return Err(MergeRefusal::NoBranch);
    }
    let ahead = git(
        root,
        repo,
        &["rev-list", "--count", &format!("HEAD..{reference}")],
    )
    .map_err(|_| MergeRefusal::GitFailed)?;
    if ahead.trim() == "0" {
        return Err(MergeRefusal::NothingToMerge);
    }
    let status = git(
        root,
        repo,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .map_err(|_| MergeRefusal::GitFailed)?;
    if !status.is_empty() {
        return Err(MergeRefusal::ParentDirty);
    }
    let message = format!("acpd: merge delegated task branch {}", worktree.branch);
    let merged = git(
        root,
        repo,
        &[
            "-c",
            "user.name=acpd",
            "-c",
            "user.email=acpd@localhost.invalid",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "--no-edit",
            "--no-verify",
            "-m",
            &message,
            &reference,
        ],
    );
    if merged.is_err() {
        let files = git(root, repo, &["diff", "--name-only", "--diff-filter=U"])
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let _ = git(root, repo, &["merge", "--abort"]);
        return Err(if files.is_empty() {
            MergeRefusal::GitFailed
        } else {
            MergeRefusal::Conflict(files)
        });
    }
    let commit = git(root, repo, &["rev-parse", "HEAD"]).map_err(|_| MergeRefusal::GitFailed)?;
    let _ = git(root, repo, &["branch", "-D", &worktree.branch]);
    Ok(commit)
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
        merged: None,
        outcome: Outcome::Active,
    };
    if !worktree.task_folder().is_dir() {
        // The folder is untracked (or empty) in the commit, so the worktree lacks it.
        let _ = std::fs::create_dir_all(worktree.task_folder());
    }
    Ok(worktree)
}

/// What to do with a finished task's uncommitted changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Uncommitted {
    /// Commit them to the task's branch (the default), then remove the worktree.
    #[default]
    Commit,
    /// Leave the worktree on disk as it is.
    Keep,
}

/// Finish with a worktree: snapshot (or keep) uncommitted changes, remove the worktree, and keep
/// its branch only if it holds new commits. Returns the outcome; an already finished worktree is
/// left alone.
pub fn release(root: &Path, worktree: &Worktree) -> Outcome {
    release_with(root, worktree, Uncommitted::Commit)
}
/// How long removing a folder is retried (Windows keeps a folder locked while a process runs in it).
pub const REMOVE_RETRY: std::time::Duration = std::time::Duration::from_secs(120);
pub fn release_with(root: &Path, worktree: &Worktree, uncommitted: Uncommitted) -> Outcome {
    release_within(root, worktree, uncommitted, REMOVE_RETRY)
}
/// As [`release_with`], but gives up removing the folder after `retry` (the caller may be
/// holding something up, such as host start-up). A folder that stays is `CleanupFailed`.
pub fn release_within(
    root: &Path,
    worktree: &Worktree,
    uncommitted: Uncommitted,
    retry: std::time::Duration,
) -> Outcome {
    if worktree.outcome != Outcome::Active {
        return worktree.outcome;
    }
    if !worktree.path.exists() {
        let _ = git(root, &worktree.repository, &["worktree", "prune"]);
        return branch_cleanup(root, worktree);
    }
    let mut snapshot = false;
    match git(
        root,
        &worktree.path,
        &["status", "--porcelain", "--untracked-files=normal"],
    ) {
        Ok(out) if !out.is_empty() => {
            if uncommitted == Uncommitted::Keep || !commit_snapshot(root, worktree) {
                return Outcome::KeptDirty;
            }
            snapshot = true;
        }
        Ok(_) => {}
        Err(_) => return Outcome::CleanupFailed,
    }
    if !remove_folder(root, worktree, retry) {
        return Outcome::CleanupFailed;
    }
    match branch_cleanup(root, worktree) {
        Outcome::BranchKept if snapshot => Outcome::Snapshotted,
        other => other,
    }
}
/// `git worktree remove`, retried (see the comment inside); true once the folder is gone.
fn remove_folder(root: &Path, worktree: &Worktree, retry: std::time::Duration) -> bool {
    let target = worktree.path.to_string_lossy().into_owned();
    // Windows keeps a folder locked while an agent process still runs in it (a cancelled task's
    // process can outlive the cancel, and a removed session's process is stopped just after its
    // tree is released), and two git commands on one repository can collide on its lock files.
    // Retry for up to `retry` (two minutes unless a caller is waiting), one release at a time per
    // host, before calling it a failure.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let deadline = std::time::Instant::now() + retry;
    let mut removed = false;
    loop {
        let attempt = {
            let _one = SERIAL.lock().unwrap_or_else(|poison| poison.into_inner());
            git(root, &worktree.repository, &["worktree", "remove", &target])
        };
        match attempt {
            Ok(_) => {
                removed = true;
                break;
            }
            Err(error) if std::time::Instant::now() < deadline && worktree.path.exists() => {
                tracing::debug!(%error, "worktree removal will be retried");
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            Err(error) => {
                tracing::warn!(%error, path = %target, "worktree removal failed");
                break;
            }
        }
    }
    removed
}
/// Commit the worktree's uncommitted work (if any) to its branch, as at release. False when
/// git failed; the worktree is then as it was.
pub fn snapshot_if_dirty(root: &Path, worktree: &Worktree) -> bool {
    match git(
        root,
        &worktree.path,
        &["status", "--porcelain", "--untracked-files=normal"],
    ) {
        Ok(out) if out.is_empty() => true,
        Ok(_) => commit_snapshot(root, worktree),
        Err(_) => false,
    }
}
/// After a merge: remove the (clean) worktree folder and the merged branch. Best effort; a
/// folder that cannot go yet (Windows, while the task's agent still runs in it) is retried and
/// otherwise left to the startup sweep. True when both are gone.
pub fn discard_merged(root: &Path, worktree: &Worktree) -> bool {
    let folder = !worktree.path.exists() || remove_folder(root, worktree, REMOVE_RETRY);
    let branch = git(
        root,
        &worktree.repository,
        &["branch", "-D", &worktree.branch],
    )
    .is_ok();
    folder && branch
}
/// Commit everything git does not ignore to the task's branch, under the host's own identity,
/// without hooks or signing. Ignored files (build output, `.env`) are not included.
fn commit_snapshot(root: &Path, worktree: &Worktree) -> bool {
    let message = format!(
        "acpd: uncommitted work of delegated task ({})",
        worktree.branch
    );
    git(root, &worktree.path, &["add", "--all"]).is_ok()
        && git(
            root,
            &worktree.path,
            &[
                "-c",
                "user.name=acpd",
                "-c",
                "user.email=acpd@localhost.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "--no-verify",
                "-m",
                &message,
            ],
        )
        .is_ok()
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
        // Windows runners default to core.autocrlf=true, which rewrites "one\n" on checkout.
        run(&repo, &["config", "core.autocrlf", "false"]);
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
        assert_eq!(
            release_with(&root, &dirty, Uncommitted::Keep),
            Outcome::KeptDirty
        );
        assert!(dirty.path.exists());
        // The default snapshots the same work to the branch and frees the folder.
        std::fs::write(dirty.path.join("untracked.txt"), "u\n").unwrap();
        assert_eq!(release(&root, &dirty), Outcome::Snapshotted);
        assert!(!dirty.path.exists());
        let shown = git(
            &root,
            &repo,
            &["show", &format!("{}:src/lib.rs", dirty.branch)],
        )
        .unwrap();
        assert_eq!(shown, "changed");
        let shown = git(
            &root,
            &repo,
            &["show", &format!("{}:untracked.txt", dirty.branch)],
        )
        .unwrap();
        assert_eq!(shown, "u");
        let author = git(&root, &repo, &["log", "-1", "--format=%an", &dirty.branch]).unwrap();
        assert_eq!(author, "acpd");
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
        // Windows runners default to core.autocrlf=true, which rewrites "one\n" on checkout.
        run(&empty, &["config", "core.autocrlf", "false"]);
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
