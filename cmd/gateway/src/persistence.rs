use conxian_core::{ConxianError, ConxianResult, Persistence, PersistentState};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicU64, atomic::Ordering, Mutex};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct FilePersistence {
    path: PathBuf,
    /// Serializes all in-process reads and writes for this shared backend.
    ///
    /// The production wiring stores one `FilePersistence` behind an `Arc`, so
    /// listeners, the orchestrator, and API handlers use the same lock.
    io_lock: Mutex<()>,
}

impl FilePersistence {
    pub fn new(path: &str) -> Self {
        Self {
            path: PathBuf::from(path),
            io_lock: Mutex::new(()),
        }
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

    fn sync_parent_directory(&self) {
        // Directory fsync is supported on Unix filesystems. Other platforms
        // may reject opening a directory as a file; that is intentionally
        // best-effort because the state-file rename has already completed.
        if let Ok(directory) = File::open(self.parent_dir()) {
            let _ = directory.sync_all();
        }
    }
}

impl Persistence for FilePersistence {
    fn save(&self, state: &PersistentState) -> ConxianResult<()> {
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| ConxianError::Internal(e.to_string()))?;
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| ConxianError::Internal("Persistence lock poisoned".to_string()))?;
        let temporary_path = self.temporary_path();
        let mut temporary_file_created = false;

        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary_path)
                .map_err(|e| ConxianError::Internal(e.to_string()))?;
            temporary_file_created = true;
            file.write_all(json.as_bytes())
                .map_err(|e| ConxianError::Internal(e.to_string()))?;
            file.flush()
                .map_err(|e| ConxianError::Internal(e.to_string()))?;
            file.sync_all()
                .map_err(|e| ConxianError::Internal(e.to_string()))?;
            drop(file);

            // On the supported Unix deployment this replaces the state path
            // atomically, so readers never observe a truncated destination.
            fs::rename(&temporary_path, &self.path)
                .map_err(|e| ConxianError::Internal(e.to_string()))?;
            self.sync_parent_directory();
            Ok(())
        })();

        if temporary_file_created && result.is_err() {
            let _ = fs::remove_file(&temporary_path);
        }

        result
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        let _guard = self
            .io_lock
            .lock()
            .map_err(|_| ConxianError::Internal("Persistence lock poisoned".to_string()))?;
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PersistentState::default());
            }
            Err(error) => return Err(ConxianError::Internal(error.to_string())),
        };
        let mut json = String::new();
        file.read_to_string(&mut json)
            .map_err(|e| ConxianError::Internal(e.to_string()))?;
        let state: PersistentState =
            serde_json::from_str(&json).map_err(|e| ConxianError::Internal(e.to_string()))?;
        Ok(state)
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
    fn missing_state_file_returns_default_state() {
        let directory = TestDirectory::new();
        let path = directory.path("missing.json");
        let path_string = path.to_string_lossy().into_owned();
        let persistence = FilePersistence::new(&path_string);

        let loaded = persistence.load().expect("missing state is not an error");

        assert_eq!(serialized(&loaded), serialized(&PersistentState::default()));
    }

    #[test]
    fn save_atomically_replaces_state_and_leaves_no_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.path("state.json");
        let path_string = path.to_string_lossy().into_owned();
        let persistence = FilePersistence::new(&path_string);
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
        assert_eq!(entries, vec![std::ffi::OsString::from("state.json")]);
    }

    #[test]
    fn save_error_cleans_up_temporary_file() {
        let directory = TestDirectory::new();
        let path = directory.path("state-target");
        fs::create_dir(&path).expect("target directory should be created");
        let path_string = path.to_string_lossy().into_owned();
        let persistence = FilePersistence::new(&path_string);

        assert!(persistence
            .save(&state_with_marker("rename-failure"))
            .is_err());

        let entries = fs::read_dir(&directory.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("state-target")]);
    }

    #[test]
    fn concurrent_loads_and_saves_only_observe_complete_states() {
        let directory = TestDirectory::new();
        let path = directory.path("concurrent.json");
        let path_string = path.to_string_lossy().into_owned();
        let persistence = Arc::new(FilePersistence::new(&path_string));
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
                        let loaded = persistence
                            .load()
                            .expect("concurrent load should see valid JSON");
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
                    persistence
                        .save(state)
                        .expect("concurrent save should succeed");
                }
            });
        });

        let final_state = persistence
            .load()
            .expect("final state should be valid JSON");
        assert!(allowed.contains(&serialized(&final_state)));
    }
}
