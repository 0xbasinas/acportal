//! Task records: what survives a host restart, and what `acpd tasks` and the phone read.
//!
//! A record says what a task was and how it ended, never what it did: no prompt, no result
//! text, no token, no request details. After a restart every task that was still open is shown
//! as failed because the host restarted. Nothing is restarted and no prompt is sent again; the
//! agent processes died with the host.
use super::tree::{Rights, TaskState};
use crate::security::{reject_symlink, write_private_json};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Most records kept. Older whole task trees are dropped first.
pub const MAX_TASKS: usize = 256;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const FILE: &str = "tasks.json";
/// Shown for a task that was open when the host stopped.
pub const INTERRUPTED: &str =
    "the host restarted; this task was not restarted and its prompt was not resent";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordRights {
    pub read: bool,
    pub write: bool,
    pub terminal: bool,
}
impl From<Rights> for RecordRights {
    fn from(rights: Rights) -> Self {
        Self {
            read: rights.read,
            write: rights.write,
            terminal: rights.terminal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskRecord {
    pub id: Uuid,
    pub parent: Option<Uuid>,
    pub root: Uuid,
    pub depth: u8,
    pub agent: String,
    pub scope: PathBuf,
    pub rights: RecordRights,
    /// `submitted`, `working`, `input_required`, `auth_required`, `completed`, `failed`,
    /// `canceled` or `rejected`.
    pub state: String,
    /// The host session this task ran in, if it got that far.
    pub session: Option<Uuid>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Why it ended badly, in the host's fixed words; never agent text.
    pub summary: Option<String>,
    pub stop_reason: Option<String>,
    /// The W3C trace id shared by every task in the tree.
    pub trace_id: Option<String>,
    /// Cumulative cost in USD, if the agent reported one. Absent means unmetered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// The task's own git worktree and what became of it, for worktree-isolated tasks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<super::worktree::Worktree>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskFile {
    version: u32,
    tasks: Vec<TaskRecord>,
}

pub fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Submitted => "submitted",
        TaskState::Working => "working",
        TaskState::InputRequired => "input_required",
        TaskState::AuthRequired => "auth_required",
        TaskState::Completed => "completed",
        TaskState::Failed => "failed",
        TaskState::Canceled => "canceled",
        TaskState::Rejected => "rejected",
    }
}
pub fn parse_state(name: &str) -> Option<TaskState> {
    Some(match name {
        "submitted" => TaskState::Submitted,
        "working" => TaskState::Working,
        "input_required" => TaskState::InputRequired,
        "auth_required" => TaskState::AuthRequired,
        "completed" => TaskState::Completed,
        "failed" => TaskState::Failed,
        "canceled" => TaskState::Canceled,
        "rejected" => TaskState::Rejected,
        _ => return None,
    })
}

pub fn path(directory: &Path) -> PathBuf {
    directory.join(FILE)
}

/// The saved records, or none if there is no file. A file that cannot be read is an error; the
/// caller decides what to do with it (the host sets it aside and starts empty).
pub fn load(directory: &Path) -> Result<Vec<TaskRecord>> {
    let file = path(directory);
    if !file.exists() {
        return Ok(vec![]);
    }
    reject_symlink(&file)?;
    let mut bytes = Vec::new();
    std::fs::File::open(&file)
        .context("open the task records")?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "the task records are too large"
    );
    let parsed: TaskFile =
        serde_json::from_slice(&bytes).context("the task records are not valid")?;
    ensure!(parsed.version == 1, "unknown task records version");
    ensure!(parsed.tasks.len() <= MAX_TASKS, "too many task records");
    for record in &parsed.tasks {
        ensure!(
            parse_state(&record.state).is_some(),
            "a task record has an unknown state"
        );
    }
    Ok(parsed.tasks)
}

pub fn save(directory: &Path, records: &[TaskRecord]) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    write_private_json(
        &path(directory),
        &TaskFile {
            version: 1,
            tasks: records.to_vec(),
        },
    )
}

/// Move an unreadable file aside so the host can start empty without losing it.
pub fn set_aside(directory: &Path) {
    let file = path(directory);
    let _ = std::fs::rename(&file, directory.join("tasks.corrupt.json"));
}

/// Every task still open becomes `failed` with the restart reason. Returns how many changed.
pub fn interrupt(records: &mut [TaskRecord], now: u64) -> usize {
    let mut changed = 0;
    for record in records.iter_mut() {
        if parse_state(&record.state).is_some_and(|state| !state.is_terminal()) {
            record.state = "failed".into();
            record.summary = Some(INTERRUPTED.into());
            record.updated_at = now;
            changed += 1;
        }
    }
    changed
}

/// Keep the newest whole task trees, up to [`MAX_TASKS`] records. A tree is never cut in half.
pub fn prune(records: Vec<TaskRecord>) -> Vec<TaskRecord> {
    if records.len() <= MAX_TASKS {
        return records;
    }
    let mut newest: HashMap<Uuid, u64> = HashMap::new();
    for record in &records {
        let entry = newest.entry(record.root).or_insert(0);
        *entry = (*entry).max(record.updated_at);
    }
    let mut roots: Vec<(Uuid, u64)> = newest.into_iter().collect();
    roots.sort_by_key(|(_, updated)| std::cmp::Reverse(*updated));
    let mut kept: Vec<Uuid> = vec![];
    let mut total = 0;
    for (root, _) in roots {
        let size = records.iter().filter(|r| r.root == root).count();
        if total + size <= MAX_TASKS {
            total += size;
            kept.push(root);
        }
    }
    records
        .into_iter()
        .filter(|record| kept.contains(&record.root))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(root: Uuid, parent: Option<Uuid>, state: &str, updated: u64) -> TaskRecord {
        TaskRecord {
            id: if parent.is_none() {
                root
            } else {
                Uuid::new_v4()
            },
            parent,
            root,
            depth: u8::from(parent.is_some()),
            agent: "worker".into(),
            scope: PathBuf::from("/w"),
            rights: RecordRights {
                read: true,
                write: false,
                terminal: false,
            },
            state: state.into(),
            session: None,
            created_at: 1,
            updated_at: updated,
            summary: None,
            stop_reason: None,
            trace_id: None,
            cost_usd: None,
            worktree: None,
        }
    }

    #[test]
    fn open_tasks_are_interrupted_and_finished_ones_are_left_alone() {
        let root = Uuid::new_v4();
        let mut records = vec![
            record(root, None, "working", 5),
            record(root, Some(root), "input_required", 5),
            record(root, Some(root), "auth_required", 5),
            record(root, Some(root), "submitted", 5),
            record(root, Some(root), "completed", 5),
            record(root, Some(root), "failed", 5),
            record(root, Some(root), "canceled", 5),
            record(root, Some(root), "rejected", 5),
        ];
        assert_eq!(interrupt(&mut records, 99), 4);
        let states: Vec<&str> = records.iter().map(|r| r.state.as_str()).collect();
        assert_eq!(
            states,
            [
                "failed",
                "failed",
                "failed",
                "failed",
                "completed",
                "failed",
                "canceled",
                "rejected"
            ]
        );
        assert!(
            records[..4]
                .iter()
                .all(|r| r.summary.as_deref() == Some(INTERRUPTED) && r.updated_at == 99)
        );
        assert!(
            records[4..]
                .iter()
                .all(|r| r.summary.is_none() && r.updated_at == 5)
        );
        assert_eq!(
            interrupt(&mut records, 100),
            0,
            "nothing is left to interrupt"
        );
    }

    #[test]
    fn pruning_keeps_the_newest_whole_trees_and_never_cuts_one() {
        let mut records = vec![];
        let mut newest = vec![];
        for tree in 0..6u64 {
            let root = Uuid::new_v4();
            records.push(record(root, None, "completed", tree * 10));
            for _ in 0..60 {
                records.push(record(root, Some(root), "completed", tree * 10));
            }
            newest.push(root);
        }
        let kept = prune(records);
        assert!(kept.len() <= MAX_TASKS);
        // 61 per tree: four whole trees fit (244); the two oldest are gone entirely.
        assert_eq!(kept.len(), 4 * 61);
        for (index, root) in newest.iter().enumerate() {
            let count = kept.iter().filter(|r| r.root == *root).count();
            assert_eq!(count, if index >= 2 { 61 } else { 0 }, "tree {index}");
        }
    }

    #[test]
    fn a_small_set_is_kept_as_it_is() {
        let root = Uuid::new_v4();
        let records = vec![record(root, None, "working", 1)];
        assert_eq!(prune(records.clone()), records);
    }

    #[test]
    fn save_and_load_round_trip_and_the_file_is_private() {
        let directory = tempfile::tempdir().unwrap();
        assert!(
            load(directory.path()).unwrap().is_empty(),
            "no file is no tasks"
        );
        let root = Uuid::new_v4();
        let records = vec![record(root, None, "failed", 7)];
        save(directory.path(), &records).unwrap();
        assert_eq!(load(directory.path()).unwrap(), records);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(path(directory.path()))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "owner only, got {mode:o}");
        }
    }

    #[test]
    fn a_damaged_file_is_refused_and_can_be_set_aside() {
        let directory = tempfile::tempdir().unwrap();
        let file = path(directory.path());
        for (label, content) in [
            ("not json", b"nonsense".to_vec()),
            (
                "unknown field",
                br#"{"version":1,"tasks":[],"extra":1}"#.to_vec(),
            ),
            ("a future version", br#"{"version":2,"tasks":[]}"#.to_vec()),
            ("too large", vec![b' '; (MAX_FILE_BYTES + 1) as usize]),
        ] {
            std::fs::write(&file, content).unwrap();
            assert!(load(directory.path()).is_err(), "{label}");
        }
        let bad_state = serde_json::json!({"version":1,"tasks":[{
            "id":Uuid::new_v4(),"parent":null,"root":Uuid::new_v4(),"depth":0,"agent":"a","scope":"/w",
            "rights":{"read":true,"write":false,"terminal":false},"state":"exploded","session":null,
            "createdAt":1,"updatedAt":1,"summary":null,"stopReason":null,"traceId":null}]});
        std::fs::write(&file, bad_state.to_string()).unwrap();
        assert!(
            load(directory.path())
                .unwrap_err()
                .to_string()
                .contains("unknown state")
        );
        set_aside(directory.path());
        assert!(!file.exists());
        assert!(
            directory.path().join("tasks.corrupt.json").exists(),
            "kept, not deleted"
        );
        assert!(load(directory.path()).unwrap().is_empty());
    }

    #[test]
    fn state_names_round_trip() {
        for state in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Rejected,
        ] {
            assert_eq!(parse_state(state_name(state)), Some(state));
        }
        assert_eq!(parse_state("nonsense"), None);
    }
}
