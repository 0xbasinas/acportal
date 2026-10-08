//! Bounded, size-rotated file sink for host lifecycle logs.
//!
//! The sink only stores records that tracing already emits; it does not add prompts, code,
//! environment values, headers or agent stderr. Each record is written whole, so rotation never
//! splits a line, and a single record is truncated to the per-file limit. Disk use is bounded by
//! `max_file_bytes * (retained_files + 1)`.

use crate::config::LoggingConfig;
use anyhow::{Context, Result, bail};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const TRUNCATED: &[u8] = b" [truncated]\n";

pub struct BoundedLog {
    state: Mutex<State>,
}

struct State {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    max_file_bytes: u64,
    retained_files: u32,
}

impl BoundedLog {
    /// Opens or creates the active file without truncating existing records.
    /// The parent directory must already exist; the host never creates log directories.
    pub fn open(config: &LoggingConfig) -> Result<Arc<Self>> {
        let path = config.file.clone().context("no log file configured")?;
        if path.is_dir() {
            bail!("log file path is a directory")
        }
        let file = open_append(&path).context("open host log file")?;
        let size = file.metadata().context("read host log file size")?.len();
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                path,
                file: Some(file),
                size,
                max_file_bytes: config.max_file_bytes,
                retained_files: config.retained_files,
            }),
        }))
    }

    /// Writes one complete record, rotating first when it would exceed the file limit.
    /// Failures are reported to the caller; the tracing writer discards them so logging
    /// can never stop the host.
    pub fn write_record(&self, record: &[u8]) -> io::Result<()> {
        if record.is_empty() {
            return Ok(());
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let limit = state.max_file_bytes as usize;
        let mut bounded;
        let record = if record.len() > limit {
            let mut end = limit.saturating_sub(TRUNCATED.len());
            while end > 0 && (record[end] & 0xC0) == 0x80 {
                end -= 1;
            }
            bounded = record[..end].to_vec();
            bounded.extend_from_slice(TRUNCATED);
            &bounded[..]
        } else {
            record
        };
        if state.size > 0 && state.size + record.len() as u64 > state.max_file_bytes {
            state.rotate()?;
        }
        if state.file.is_none() {
            let file = open_append(&state.path)?;
            state.size = file.metadata()?.len();
            state.file = Some(file);
        }
        let file = state.file.as_mut().expect("log file opened above");
        file.write_all(record)?;
        state.size += record.len() as u64;
        Ok(())
    }

    pub fn writer(self: &Arc<Self>) -> LogWriter {
        LogWriter(self.clone())
    }
}

impl State {
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let _ = std::fs::remove_file(rotated(&self.path, self.retained_files));
        for index in (1..self.retained_files).rev() {
            let from = rotated(&self.path, index);
            if from.exists() {
                std::fs::rename(&from, rotated(&self.path, index + 1))?;
            }
        }
        match std::fs::rename(&self.path, rotated(&self.path, 1)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
        self.size = 0;
        Ok(())
    }
}

/// Path of the rotated file with the given index, for example `acpd.log.1`.
pub fn rotated(path: &Path, index: u32) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{index}"));
    path.with_file_name(name)
}

fn open_append(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let created = !path.exists();
    let file = options.open(path)?;
    if created {
        crate::security::protect(path, false).map_err(io::Error::other)?;
    }
    Ok(file)
}

/// `tracing_subscriber` writer factory. Each event is buffered and written as one record.
#[derive(Clone)]
pub struct LogWriter(Arc<BoundedLog>);

pub struct RecordWriter {
    log: Arc<BoundedLog>,
    buffer: Vec<u8>,
}

impl Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RecordWriter {
    fn drop(&mut self) {
        let _ = self.log.write_record(&self.buffer);
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = RecordWriter;
    fn make_writer(&'a self) -> Self::Writer {
        RecordWriter {
            log: self.0.clone(),
            buffer: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(path: &Path, max_file_bytes: u64, retained_files: u32) -> LoggingConfig {
        LoggingConfig {
            file: Some(path.to_owned()),
            max_file_bytes,
            retained_files,
            ..Default::default()
        }
    }

    fn total_bytes(directory: &Path) -> u64 {
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum()
    }

    #[test]
    fn rotates_whole_records_and_bounds_retained_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("acpd.log");
        let log = BoundedLog::open(&config(&path, 100, 2)).unwrap();
        for index in 0..40 {
            log.write_record(format!("record {index:02} padding-padding\n").as_bytes())
                .unwrap();
        }
        let mut names: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["acpd.log", "acpd.log.1", "acpd.log.2"]);
        assert!(total_bytes(directory.path()) <= 300);
        for name in &names {
            let text = std::fs::read_to_string(directory.path().join(name)).unwrap();
            assert!(text.len() <= 100);
            assert!(text.lines().all(|line| line.ends_with("padding-padding")));
        }
        let newest = std::fs::read_to_string(&path).unwrap();
        assert!(newest.ends_with("record 39 padding-padding\n"));
        let older = std::fs::read_to_string(rotated(&path, 1)).unwrap();
        assert!(older.contains("record 3"));
        assert!(!older.contains("record 39"));
    }

    #[test]
    fn reopening_appends_and_oversized_records_are_truncated_on_char_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("acpd.log");
        std::fs::write(&path, b"existing\n").unwrap();
        let log = BoundedLog::open(&config(&path, 64, 1)).unwrap();
        log.write_record(b"next\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"existing\nnext\n");
        let long = "αβγ".repeat(40) + "\n";
        log.write_record(long.as_bytes()).unwrap();
        assert_eq!(
            std::fs::read(rotated(&path, 1)).unwrap(),
            b"existing\nnext\n"
        );
        let truncated = std::fs::read_to_string(&path).unwrap();
        assert!(truncated.len() <= 64);
        assert!(truncated.ends_with(" [truncated]\n"));
        assert!(truncated.starts_with("αβγ"));
    }

    #[test]
    fn missing_parent_is_not_created_and_directory_path_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("logs").join("acpd.log");
        assert!(BoundedLog::open(&config(&missing, 1024, 1)).is_err());
        assert!(!directory.path().join("logs").exists());
        assert!(BoundedLog::open(&config(directory.path(), 1024, 1)).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn new_log_files_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("acpd.log");
        let log = BoundedLog::open(&config(&path, 64, 1)).unwrap();
        for _ in 0..4 {
            log.write_record(b"0123456789abcdefghij0123456789\n")
                .unwrap();
        }
        for file in [path.clone(), rotated(&path, 1)] {
            let mode = std::fs::metadata(file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn tracing_events_reach_the_bounded_file() {
        use tracing_subscriber::layer::SubscriberExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("acpd.log");
        let log = BoundedLog::open(&config(&path, 64 * 1024, 1)).unwrap();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_target(false)
                .with_writer(log.writer()),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(session_id = "fixture", "session created");
        });
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("session created"));
        assert!(text.contains("session_id=\"fixture\""));
        assert_eq!(text.lines().count(), 1);
    }
}
