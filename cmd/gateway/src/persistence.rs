use conxian_core::{
    ConxianError, ConxianResult, Persistence, PersistentState, VersionedPersistentState,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicU64, atomic::Ordering, Mutex};

const STATE_FORMAT_VERSION: u32 = 1;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
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

pub struct FilePersistence {
    path: PathBuf,
    transaction_lock_path: PathBuf,
    ownership_lock_path: PathBuf,
    /// Serializes filesystem lock acquisition for callers sharing this backend.
    io_lock: Mutex<()>,
}

impl FilePersistence {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            transaction_lock_path: Self::sibling_lock_path(&path, "transaction"),
            ownership_lock_path: Self::sibling_lock_path(&path, "ownership"),
            path,
            io_lock: Mutex::new(()),
        }
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

        if value.get("format_version").is_some() {
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
            self.sync_parent_directory()?;
            Ok(())
        })();

        if temporary_file_created && result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }

    #[cfg(unix)]
    fn sync_parent_directory(&self) -> ConxianResult<()> {
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
    fn save(&self, state: &PersistentState) -> ConxianResult<()> {
        let current = self.load_versioned()?;
        self.compare_and_swap(current.revision, state).map(|_| ())
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        self.load_versioned().map(|versioned| versioned.state)
    }

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
    use conxian_core::{Persistence, TrackedMempoolTx};
    use std::sync::{atomic::AtomicU64, Arc, Barrier};
    use std::thread;

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

    #[test]
    fn missing_state_file_returns_default_revision_zero() {
        let directory = TestDirectory::new();
        let persistence = FilePersistence::new(directory.path("missing.json"));
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
        let persistence = FilePersistence::new(&path);
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
        let persistence = FilePersistence::new(directory.path("state.json"));
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
        let persistence = FilePersistence::new(directory.path("state.json"));
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
    fn separately_constructed_backends_serialize_competing_transactions() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let first = FilePersistence::new(&path);
        let second = FilePersistence::new(&path);
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
        assert_eq!(
            FilePersistence::new(&path)
                .load_versioned()
                .unwrap()
                .revision,
            1
        );
    }

    #[test]
    fn corrupt_and_unsupported_state_files_are_rejected() {
        let directory = TestDirectory::new();
        let corrupt_path = directory.path("corrupt.json");
        fs::write(&corrupt_path, b"{not-json").unwrap();
        let corrupt_error = FilePersistence::new(&corrupt_path)
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
        let unsupported_error = FilePersistence::new(&unsupported_path)
            .load_versioned()
            .expect_err("unsupported format must fail closed");
        assert!(unsupported_error
            .to_string()
            .contains("unsupported persistence format version"));
    }

    #[test]
    fn second_ownership_guard_fails_closed() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let first = FilePersistence::new(&path);
        let second = FilePersistence::new(&path);
        let _guard = first.acquire_ownership().unwrap();
        let error = second
            .acquire_ownership()
            .expect_err("second owner must not acquire the state path");
        assert!(error.to_string().contains("already owned"));
    }

    #[test]
    fn inaccessible_lock_paths_fail_closed() {
        let directory = TestDirectory::new();
        let persistence = FilePersistence::new(directory.path("state.json"));
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
    fn save_atomically_replaces_state_and_leaves_no_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let persistence = FilePersistence::new(&path);
        let expected = state_with_marker("complete-state");
        persistence
            .save(&expected)
            .expect("state save should succeed");
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
    fn save_error_cleans_up_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.path("state-target");
        fs::create_dir(&path).expect("target directory should be created");
        let persistence = FilePersistence::new(&path);
        assert!(persistence
            .save(&state_with_marker("rename-failure"))
            .is_err());
        let entries = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert!(!entries
            .iter()
            .any(|name| name.to_string_lossy().contains(".tmp-")));
    }

    #[test]
    fn concurrent_loads_and_saves_only_observe_complete_states() {
        let directory = TestDirectory::new();
        let path = directory.path("concurrent.json");
        let persistence = Arc::new(FilePersistence::new(&path));
        let first = state_with_marker("first-complete-state");
        let second = state_with_marker("second-complete-state");
        let allowed = [serialized(&first), serialized(&second)];
        persistence
            .save(&first)
            .expect("initial state save should succeed");

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
                    persistence.save(state).expect("concurrent save succeeds");
                }
            });
        });
        let final_state = persistence.load().expect("final state valid");
        assert!(allowed.contains(&serialized(&final_state)));
    }
}
