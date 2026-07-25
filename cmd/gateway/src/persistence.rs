use conxian_core::{
    ConxianError, ConxianResult, Persistence, PersistentState, VersionedPersistentState,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicBool;
use std::sync::{atomic::AtomicU64, atomic::Ordering, Mutex};

const STATE_FORMAT_VERSION: u32 = 1;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEnvelope {
    format_version: u32,
    revision: u64,
    state: PersistentState,
}

#[derive(Debug)]
pub struct StateOwnershipGuard {
    lock_file: File,
}

impl Drop for StateOwnershipGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.lock_file);
    }
}

#[derive(Debug)]
pub struct FilePersistence {
    path: PathBuf,
    transaction_lock_path: PathBuf,
    ownership_lock_path: PathBuf,
    /// Serializes filesystem lock acquisition for callers sharing this backend.
    io_lock: Mutex<()>,
    #[cfg(test)]
    fail_parent_sync_once: AtomicBool,
}

impl FilePersistence {
    pub fn new(path: impl Into<PathBuf>) -> ConxianResult<Self> {
        let configured_path = path.into();
        let absolute_path = if configured_path.is_absolute() {
            configured_path
        } else {
            std::env::current_dir()
                .map_err(|error| {
                    ConxianError::Persistence(format!(
                        "failed to resolve current directory for persistence path: {error}"
                    ))
                })?
                .join(configured_path)
        };
        let file_name = absolute_path.file_name().ok_or_else(|| {
            ConxianError::Persistence(format!(
                "persistence path '{}' has no file name",
                absolute_path.display()
            ))
        })?;
        let configured_parent = absolute_path.parent().ok_or_else(|| {
            ConxianError::Persistence(format!(
                "persistence path '{}' has no parent directory",
                absolute_path.display()
            ))
        })?;
        let parent = fs::canonicalize(configured_parent).map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to canonicalize persistence parent '{}': {error}",
                configured_parent.display()
            ))
        })?;
        let path = parent.join(file_name);
        Self::validate_state_path(&path)?;

        Ok(Self {
            transaction_lock_path: Self::sibling_lock_path(&path, "transaction"),
            ownership_lock_path: Self::sibling_lock_path(&path, "ownership"),
            path,
            io_lock: Mutex::new(()),
            #[cfg(test)]
            fail_parent_sync_once: AtomicBool::new(false),
        })
    }

    fn validate_state_path(path: &Path) -> ConxianResult<()> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(ConxianError::Persistence(format!(
                    "failed to inspect state path '{}': {error}",
                    path.display()
                )));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(ConxianError::Persistence(format!(
                "state path '{}' must be a regular non-symlink file",
                path.display()
            )));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 {
                return Err(ConxianError::Persistence(format!(
                    "state path '{}' must not be hard-linked",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    /// Acquire exclusive ownership of this state path for the process lifetime.
    pub fn acquire_ownership(&self) -> ConxianResult<StateOwnershipGuard> {
        let lock_file = self.open_lock_file(&self.ownership_lock_path)?;
        lock_file.try_lock_exclusive().map_err(|error| {
            ConxianError::Persistence(format!(
                "state path '{}' is already owned by another Gateway process or the ownership lock cannot be acquired: {error}",
                self.path.display()
            ))
        })?;
        Ok(StateOwnershipGuard { lock_file })
    }

    fn sibling_lock_path(path: &Path, purpose: &str) -> PathBuf {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("gateway-state");
        parent.join(format!(".{file_name}.{purpose}.lock"))
    }

    fn parent_dir(&self) -> &Path {
        self.path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }

    fn temporary_path(&self) -> PathBuf {
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("gateway-state");
        let sequence = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.parent_dir().join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ))
    }

    fn open_lock_file(&self, path: &Path) -> ConxianResult<File> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| {
                ConxianError::Persistence(format!(
                    "failed to open persistence lock '{}': {error}",
                    path.display()
                ))
            })
    }

    fn with_transaction_lock<T>(
        &self,
        operation: impl FnOnce() -> ConxianResult<T>,
    ) -> ConxianResult<T> {
        let _guard = self.io_lock.lock().map_err(|_| {
            ConxianError::Persistence("in-process persistence lock poisoned".to_string())
        })?;
        let lock_file = self.open_lock_file(&self.transaction_lock_path)?;
        lock_file.lock_exclusive().map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to acquire transaction lock '{}': {error}",
                self.transaction_lock_path.display()
            ))
        })?;

        let result = operation();
        let unlock_result = FileExt::unlock(&lock_file).map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to release transaction lock '{}': {error}",
                self.transaction_lock_path.display()
            ))
        });
        match (result, unlock_result) {
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    }

    fn read_versioned_unlocked(&self) -> ConxianResult<VersionedPersistentState> {
        Self::validate_state_path(&self.path)?;
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(VersionedPersistentState {
                    revision: 0,
                    state: PersistentState::default(),
                });
            }
            Err(error) => {
                return Err(ConxianError::Persistence(format!(
                    "failed to open state file '{}': {error}",
                    self.path.display()
                )));
            }
        };
        let mut json = String::new();
        file.read_to_string(&mut json).map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to read state file '{}': {error}",
                self.path.display()
            ))
        })?;
        let value: serde_json::Value = serde_json::from_str(&json).map_err(|error| {
            ConxianError::Persistence(format!(
                "state file '{}' contains invalid JSON: {error}",
                self.path.display()
            ))
        })?;

        let is_envelope_like = ["format_version", "revision", "state"]
            .iter()
            .any(|key| value.get(key).is_some());
        if is_envelope_like {
            let envelope: StateEnvelope = serde_json::from_value(value).map_err(|error| {
                ConxianError::Persistence(format!(
                    "state file '{}' contains an invalid persistence envelope: {error}",
                    self.path.display()
                ))
            })?;
            if envelope.format_version != STATE_FORMAT_VERSION {
                return Err(ConxianError::Persistence(format!(
                    "unsupported persistence format version {} in '{}'; supported version is {STATE_FORMAT_VERSION}",
                    envelope.format_version,
                    self.path.display()
                )));
            }
            return Ok(VersionedPersistentState {
                revision: envelope.revision,
                state: envelope.state,
            });
        }

        let state: PersistentState = serde_json::from_value(value).map_err(|error| {
            ConxianError::Persistence(format!(
                "state file '{}' is neither a supported envelope nor legacy state: {error}",
                self.path.display()
            ))
        })?;
        Ok(VersionedPersistentState { revision: 0, state })
    }

    fn write_envelope_unlocked(&self, envelope: &StateEnvelope) -> ConxianResult<()> {
        Self::validate_state_path(&self.path)?;
        let json = serde_json::to_string_pretty(envelope).map_err(|error| {
            ConxianError::Persistence(format!("failed to serialize state envelope: {error}"))
        })?;
        let temporary_path = self.temporary_path();
        let mut temporary_file_created = false;
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)
                .map_err(|error| {
                    ConxianError::Persistence(format!(
                        "failed to create temporary state file '{}': {error}",
                        temporary_path.display()
                    ))
                })?;
            temporary_file_created = true;
            file.write_all(json.as_bytes()).map_err(|error| {
                ConxianError::Persistence(format!(
                    "failed to write temporary state file '{}': {error}",
                    temporary_path.display()
                ))
            })?;
            file.flush().map_err(|error| {
                ConxianError::Persistence(format!(
                    "failed to flush temporary state file '{}': {error}",
                    temporary_path.display()
                ))
            })?;
            file.sync_all().map_err(|error| {
                ConxianError::Persistence(format!(
                    "failed to sync temporary state file '{}': {error}",
                    temporary_path.display()
                ))
            })?;
            drop(file);
            fs::rename(&temporary_path, &self.path).map_err(|error| {
                ConxianError::Persistence(format!(
                    "failed to atomically replace state file '{}': {error}",
                    self.path.display()
                ))
            })?;
            self.sync_parent_directory().map_err(|error| {
                ConxianError::PersistenceCommitUnknown {
                    revision: envelope.revision,
                    message: error.to_string(),
                }
            })?;
            Ok(())
        })();

        if temporary_file_created && result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    #[cfg(unix)]
    fn sync_parent_directory(&self) -> ConxianResult<()> {
        #[cfg(test)]
        if self.fail_parent_sync_once.swap(false, Ordering::SeqCst) {
            return Err(ConxianError::Persistence(
                "injected parent directory sync failure".to_string(),
            ));
        }
        let directory = File::open(self.parent_dir()).map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to open state parent directory '{}': {error}",
                self.parent_dir().display()
            ))
        })?;
        directory.sync_all().map_err(|error| {
            ConxianError::Persistence(format!(
                "failed to sync state parent directory '{}': {error}",
                self.parent_dir().display()
            ))
        })
    }

    #[cfg(not(unix))]
    fn sync_parent_directory(&self) -> ConxianResult<()> {
        Ok(())
    }
}

impl Persistence for FilePersistence {
    fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
        self.with_transaction_lock(|| self.read_versioned_unlocked())
    }

    fn compare_and_swap(
        &self,
        expected_revision: u64,
        new_state: &PersistentState,
    ) -> ConxianResult<VersionedPersistentState> {
        self.with_transaction_lock(|| {
            let current = self.read_versioned_unlocked()?;
            if current.revision != expected_revision {
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            let revision = current.revision.checked_add(1).ok_or_else(|| {
                ConxianError::Persistence("persistence revision overflow".to_string())
            })?;
            let envelope = StateEnvelope {
                format_version: STATE_FORMAT_VERSION,
                revision,
                state: new_state.clone(),
            };
            self.write_envelope_unlocked(&envelope)?;
            Ok(VersionedPersistentState {
                revision,
                state: new_state.clone(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conxian_core::{transactional_update, Persistence, TrackedMempoolTx};
    use std::process::Command;
    use std::sync::{atomic::AtomicU64, Arc, Barrier};
    use std::thread;
    use std::time::{Duration, Instant};

    static TEST_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            loop {
                let sequence = TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "conxian-gateway-persistence-test-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create test directory: {error}"),
                }
            }
        }

        fn path(&self, file_name: &str) -> PathBuf {
            self.0.join(file_name)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn state_with_marker(marker: &str) -> PersistentState {
        let mempool_pending_txs = (0..32)
            .map(|index| TrackedMempoolTx {
                txid: format!("{marker}-{index}"),
                ..TrackedMempoolTx::default()
            })
            .collect();
        PersistentState {
            bitcoin_height: marker.len() as u64,
            stacks_height: marker.len() as u64 + 1,
            mempool_pending_txs,
        }
    }

    fn serialized(state: &PersistentState) -> String {
        serde_json::to_string(state).expect("test state should serialize")
    }

    fn open(path: impl Into<PathBuf>) -> FilePersistence {
        FilePersistence::new(path).expect("test persistence path should be valid")
    }

    #[test]
    fn missing_state_file_returns_default_revision_zero() {
        let directory = TestDirectory::new();
        let persistence = open(directory.path("missing.json"));
        let loaded = persistence
            .load_versioned()
            .expect("missing state is valid");
        assert_eq!(loaded.revision, 0);
        assert_eq!(
            serialized(&loaded.state),
            serialized(&PersistentState::default())
        );
    }

    #[test]
    fn legacy_state_migrates_to_versioned_envelope_on_first_mutation() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let legacy = state_with_marker("legacy");
        fs::write(&path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        let persistence = open(&path);
        let loaded = persistence.load_versioned().unwrap();
        assert_eq!(loaded.revision, 0);
        assert_eq!(serialized(&loaded.state), serialized(&legacy));

        let replacement = state_with_marker("migrated");
        let updated = persistence.compare_and_swap(0, &replacement).unwrap();
        assert_eq!(updated.revision, 1);
        let envelope: StateEnvelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(envelope.format_version, STATE_FORMAT_VERSION);
        assert_eq!(envelope.revision, 1);
        assert_eq!(serialized(&envelope.state), serialized(&replacement));
    }

    #[test]
    fn revisions_increment_monotonically() {
        let directory = TestDirectory::new();
        let persistence = open(directory.path("state.json"));
        let first = persistence
            .compare_and_swap(0, &state_with_marker("first"))
            .unwrap();
        let second = persistence
            .compare_and_swap(first.revision, &state_with_marker("second"))
            .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        assert_eq!(persistence.load_versioned().unwrap().revision, 2);
    }

    #[test]
    fn stale_revision_conflict_preserves_committed_state() {
        let directory = TestDirectory::new();
        let persistence = open(directory.path("state.json"));
        let initial = persistence.load_versioned().unwrap();
        let committed = state_with_marker("committed");
        persistence
            .compare_and_swap(initial.revision, &committed)
            .unwrap();
        let error = persistence
            .compare_and_swap(initial.revision, &state_with_marker("stale"))
            .expect_err("stale update must fail");
        assert!(matches!(
            error,
            ConxianError::PersistenceConflict {
                expected: 0,
                actual: 1
            }
        ));
        let final_state = persistence.load_versioned().unwrap();
        assert_eq!(final_state.revision, 1);
        assert_eq!(serialized(&final_state.state), serialized(&committed));
    }

    #[test]
    fn post_rename_sync_failure_reports_unknown_commit_without_retrying() {
        let directory = TestDirectory::new();
        let persistence = open(directory.path("state.json"));
        persistence
            .fail_parent_sync_once
            .store(true, Ordering::SeqCst);
        let error = persistence
            .compare_and_swap(0, &state_with_marker("renamed"))
            .expect_err("post-rename sync failure must be ambiguous");
        assert!(matches!(
            error,
            ConxianError::PersistenceCommitUnknown { revision: 1, .. }
        ));
        let committed = persistence.load_versioned().unwrap();
        assert_eq!(committed.revision, 1);
        assert_eq!(
            serialized(&committed.state),
            serialized(&state_with_marker("renamed"))
        );
    }

    #[test]
    fn separately_constructed_backends_serialize_competing_transactions() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let first = open(&path);
        let second = open(&path);
        let barrier = Arc::new(Barrier::new(3));
        let first_barrier = Arc::clone(&barrier);
        let first_thread = thread::spawn(move || {
            first_barrier.wait();
            first.compare_and_swap(0, &state_with_marker("first"))
        });
        let second_barrier = Arc::clone(&barrier);
        let second_thread = thread::spawn(move || {
            second_barrier.wait();
            second.compare_and_swap(0, &state_with_marker("second"))
        });
        barrier.wait();
        let results = [first_thread.join().unwrap(), second_thread.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        assert_eq!(open(&path).load_versioned().unwrap().revision, 1);
    }

    #[test]
    fn corrupt_and_unsupported_state_files_are_rejected() {
        let directory = TestDirectory::new();
        let corrupt_path = directory.path("corrupt.json");
        fs::write(&corrupt_path, b"{not-json").unwrap();
        let corrupt_error = open(&corrupt_path)
            .load_versioned()
            .expect_err("corrupt state must fail closed");
        assert!(corrupt_error.to_string().contains("invalid JSON"));

        let unsupported_path = directory.path("unsupported.json");
        fs::write(
            &unsupported_path,
            serde_json::json!({
                "format_version": STATE_FORMAT_VERSION + 1,
                "revision": 9,
                "state": PersistentState::default(),
            })
            .to_string(),
        )
        .unwrap();
        let unsupported_error = open(&unsupported_path)
            .load_versioned()
            .expect_err("unsupported format must fail closed");
        assert!(unsupported_error
            .to_string()
            .contains("unsupported persistence format version"));
    }

    #[test]
    fn malformed_and_mixed_envelope_shapes_are_rejected() {
        let directory = TestDirectory::new();
        let path = directory.path("malformed.json");
        let cases = [
            serde_json::json!({"format_version": 1}),
            serde_json::json!({"revision": 1}),
            serde_json::json!({"state": PersistentState::default()}),
            serde_json::json!({
                "format_version": 1,
                "revision": 1,
                "state": PersistentState::default(),
                "unexpected": true
            }),
            serde_json::json!({
                "bitcoin_height": 1,
                "stacks_height": 2,
                "mempool_pending_txs": [],
                "revision": 3
            }),
            serde_json::json!({
                "bitcoin_height": 1,
                "stacks_height": 2,
                "mempool_pending_txs": [],
                "unexpected": true
            }),
        ];
        for value in cases {
            fs::write(&path, value.to_string()).unwrap();
            assert!(open(&path).load_versioned().is_err(), "accepted {value}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_hard_link_state_paths_are_rejected() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.path("target.json");
        fs::write(
            &target,
            serde_json::to_vec(&PersistentState::default()).unwrap(),
        )
        .unwrap();
        let symlink_path = directory.path("symlink.json");
        symlink(&target, &symlink_path).unwrap();
        assert!(FilePersistence::new(&symlink_path).is_err());

        let hard_link_path = directory.path("hard-link.json");
        fs::hard_link(&target, &hard_link_path).unwrap();
        assert!(FilePersistence::new(&hard_link_path).is_err());
    }

    #[test]
    fn relative_and_absolute_paths_share_one_normalized_ownership_identity() {
        let current = std::env::current_dir().unwrap();
        let directory = current.join(format!(
            ".conxian-persistence-relative-test-{}-{}",
            std::process::id(),
            TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let _cleanup = TestDirectory(directory.clone());
        let absolute = directory.join("state.json");
        let relative = absolute.strip_prefix(&current).unwrap();
        let absolute_backend = open(&absolute);
        let relative_backend = open(relative);
        assert_eq!(absolute_backend.path, relative_backend.path);
        let _guard = absolute_backend.acquire_ownership().unwrap();
        assert!(relative_backend.acquire_ownership().is_err());
    }

    #[test]
    fn subprocess_worker() {
        let Ok(mode) = std::env::var("CONXIAN_PERSISTENCE_SUBPROCESS_MODE") else {
            return;
        };
        let path = PathBuf::from(std::env::var("CONXIAN_PERSISTENCE_STATE_PATH").unwrap());
        let output = PathBuf::from(std::env::var("CONXIAN_PERSISTENCE_OUTPUT_PATH").unwrap());
        let persistence = open(&path);
        match mode.as_str() {
            "ownership" => {
                let result = if persistence.acquire_ownership().is_ok() {
                    "acquired"
                } else {
                    "blocked"
                };
                fs::write(output, result).unwrap();
            }
            "cas" => {
                let ready = PathBuf::from(std::env::var("CONXIAN_PERSISTENCE_READY_PATH").unwrap());
                let go = PathBuf::from(std::env::var("CONXIAN_PERSISTENCE_GO_PATH").unwrap());
                fs::write(ready, b"ready").unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                while !go.exists() {
                    assert!(
                        Instant::now() < deadline,
                        "timed out waiting for CAS barrier"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                let marker = std::env::var("CONXIAN_PERSISTENCE_MARKER").unwrap();
                let result = match persistence.compare_and_swap(0, &state_with_marker(&marker)) {
                    Ok(_) => "committed",
                    Err(ConxianError::PersistenceConflict { .. }) => "conflict",
                    Err(error) => panic!("unexpected subprocess CAS error: {error}"),
                };
                fs::write(output, result).unwrap();
            }
            other => panic!("unsupported subprocess mode {other}"),
        }
    }

    fn spawn_worker(
        mode: &str,
        state_path: &Path,
        output_path: &Path,
        extra_env: &[(&str, String)],
    ) -> std::process::Child {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--exact")
            .arg("persistence::tests::subprocess_worker")
            .arg("--nocapture")
            .env("CONXIAN_PERSISTENCE_SUBPROCESS_MODE", mode)
            .env("CONXIAN_PERSISTENCE_STATE_PATH", state_path)
            .env("CONXIAN_PERSISTENCE_OUTPUT_PATH", output_path)
            .env("RUST_TEST_THREADS", "1");
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command.spawn().unwrap()
    }

    #[test]
    fn subprocess_ownership_exclusion_and_release() {
        let directory = TestDirectory::new();
        let state_path = directory.path("state.json");
        let first_output = directory.path("first.out");
        let second_output = directory.path("second.out");
        let persistence = open(&state_path);
        let guard = persistence.acquire_ownership().unwrap();

        let status = spawn_worker("ownership", &state_path, &first_output, &[])
            .wait()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_to_string(&first_output).unwrap(), "blocked");

        drop(guard);
        let status = spawn_worker("ownership", &state_path, &second_output, &[])
            .wait()
            .unwrap();
        assert!(status.success());
        assert_eq!(fs::read_to_string(&second_output).unwrap(), "acquired");
    }

    #[test]
    fn subprocess_same_revision_cas_has_one_winner() {
        let directory = TestDirectory::new();
        let state_path = directory.path("state.json");
        let go = directory.path("go");
        let ready_a = directory.path("ready-a");
        let ready_b = directory.path("ready-b");
        let output_a = directory.path("a.out");
        let output_b = directory.path("b.out");
        let mut first = spawn_worker(
            "cas",
            &state_path,
            &output_a,
            &[
                (
                    "CONXIAN_PERSISTENCE_READY_PATH",
                    ready_a.display().to_string(),
                ),
                ("CONXIAN_PERSISTENCE_GO_PATH", go.display().to_string()),
                ("CONXIAN_PERSISTENCE_MARKER", "process-a".to_string()),
            ],
        );
        let mut second = spawn_worker(
            "cas",
            &state_path,
            &output_b,
            &[
                (
                    "CONXIAN_PERSISTENCE_READY_PATH",
                    ready_b.display().to_string(),
                ),
                ("CONXIAN_PERSISTENCE_GO_PATH", go.display().to_string()),
                ("CONXIAN_PERSISTENCE_MARKER", "process-b".to_string()),
            ],
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !(ready_a.exists() && ready_b.exists()) {
            assert!(
                Instant::now() < deadline,
                "subprocesses did not reach barrier"
            );
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(&go, b"go").unwrap();
        assert!(first.wait().unwrap().success());
        assert!(second.wait().unwrap().success());
        let outcomes = [
            fs::read_to_string(output_a).unwrap(),
            fs::read_to_string(output_b).unwrap(),
        ];
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| *outcome == "committed")
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| *outcome == "conflict")
                .count(),
            1
        );
        assert_eq!(open(&state_path).load_versioned().unwrap().revision, 1);
    }

    #[test]
    fn second_ownership_guard_fails_closed() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let first = open(&path);
        let second = open(&path);
        let _guard = first.acquire_ownership().unwrap();
        let error = second
            .acquire_ownership()
            .expect_err("second owner must not acquire the state path");
        assert!(error.to_string().contains("already owned"));
    }

    #[test]
    fn inaccessible_lock_paths_fail_closed() {
        let directory = TestDirectory::new();
        let persistence = open(directory.path("state.json"));
        fs::create_dir(&persistence.transaction_lock_path).unwrap();
        fs::create_dir(&persistence.ownership_lock_path).unwrap();

        let transaction_error = persistence
            .load_versioned()
            .expect_err("transaction lock open failure must be returned");
        assert!(transaction_error
            .to_string()
            .contains("failed to open persistence lock"));

        let ownership_error = persistence
            .acquire_ownership()
            .expect_err("ownership lock open failure must be returned");
        assert!(ownership_error
            .to_string()
            .contains("failed to open persistence lock"));
    }

    #[test]
    fn cas_atomically_replaces_state_and_leaves_no_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let persistence = open(&path);
        let expected = state_with_marker("complete-state");
        persistence
            .compare_and_swap(0, &expected)
            .expect("state CAS should succeed");
        assert_eq!(
            serialized(&persistence.load().unwrap()),
            serialized(&expected)
        );
        let entries = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert!(!entries
            .iter()
            .any(|name| name.to_string_lossy().contains(".tmp-")));
    }

    #[test]
    fn non_regular_state_path_is_rejected() {
        let directory = TestDirectory::new();
        let path = directory.path("state-target");
        fs::create_dir(&path).expect("target directory should be created");
        let error = FilePersistence::new(&path).expect_err("directory target must fail");
        assert!(error.to_string().contains("regular non-symlink file"));
    }

    #[test]
    fn concurrent_loads_and_transactions_only_observe_complete_states() {
        let directory = TestDirectory::new();
        let path = directory.path("concurrent.json");
        let persistence = Arc::new(open(&path));
        let first = state_with_marker("first-complete-state");
        let second = state_with_marker("second-complete-state");
        let allowed = [serialized(&first), serialized(&second)];
        persistence
            .compare_and_swap(0, &first)
            .expect("initial state CAS should succeed");

        let barrier = Arc::new(Barrier::new(5));
        thread::scope(|scope| {
            for _ in 0..4 {
                let persistence = Arc::clone(&persistence);
                let barrier = Arc::clone(&barrier);
                let allowed = allowed.clone();
                scope.spawn(move || {
                    barrier.wait();
                    for _ in 0..128 {
                        let loaded = persistence.load().expect("load valid JSON");
                        assert!(allowed.contains(&serialized(&loaded)));
                    }
                });
            }
            let persistence = Arc::clone(&persistence);
            let barrier = Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                for iteration in 0..128 {
                    let state = if iteration % 2 == 0 { &first } else { &second };
                    transactional_update(persistence.as_ref(), 8, |next| {
                        *next = state.clone();
                        Ok(())
                    })
                    .expect("concurrent transaction succeeds");
                }
            });
        });
        let final_state = persistence.load().expect("final state valid");
        assert!(allowed.contains(&serialized(&final_state)));
    }
}
