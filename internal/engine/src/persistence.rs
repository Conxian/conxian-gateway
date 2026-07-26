use conxian_core::{
    transactional_update, ConxianError, ConxianResult, Persistence, PersistentState,
    VersionedPersistentState,
};
use std::sync::Arc;

/// Async boundary for persistence implementations whose durable operations are
/// synchronous and may block on filesystem locks or storage I/O.
#[derive(Clone)]
pub struct AsyncPersistence {
    inner: Arc<dyn Persistence>,
}

impl AsyncPersistence {
    pub fn new(inner: Arc<dyn Persistence>) -> Self {
        Self { inner }
    }

    pub async fn load(&self) -> ConxianResult<PersistentState> {
        let persistence = self.inner.clone();
        run_blocking_persistence("load", move || persistence.load()).await
    }

    pub async fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
        let persistence = self.inner.clone();
        run_blocking_persistence("load versioned state", move || persistence.load_versioned()).await
    }

    pub async fn compare_and_swap(
        &self,
        expected_revision: u64,
        new_state: PersistentState,
    ) -> ConxianResult<VersionedPersistentState> {
        let persistence = self.inner.clone();
        run_blocking_persistence("compare and swap", move || {
            persistence.compare_and_swap(expected_revision, &new_state)
        })
        .await
    }

    /// Run the complete bounded transaction on one blocking-pool task. This
    /// keeps every synchronous load/CAS retry off Tokio worker threads while
    /// preserving the core helper's conflict-only retry semantics.
    pub async fn transactional_update<T, F>(
        &self,
        max_attempts: usize,
        mutate: F,
    ) -> ConxianResult<(VersionedPersistentState, T)>
    where
        T: Send + 'static,
        F: FnMut(&mut PersistentState) -> ConxianResult<T> + Send + 'static,
    {
        let persistence = self.inner.clone();
        run_blocking_persistence("transactional update", move || {
            transactional_update(persistence.as_ref(), max_attempts, mutate)
        })
        .await
    }
}

pub async fn run_blocking_persistence<T, F>(operation: &'static str, task: F) -> ConxianResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ConxianResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(task).await.map_err(|error| {
        ConxianError::Persistence(format!(
            "blocking persistence task '{operation}' failed: {error}"
        ))
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{mpsc, Mutex};
    use std::thread::ThreadId;
    use std::time::Duration;

    struct ThreadRecordingPersistence {
        operation_thread: Mutex<Option<ThreadId>>,
        started: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl Persistence for ThreadRecordingPersistence {
        fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
            *self.operation_thread.lock().expect("lock poisoned") =
                Some(std::thread::current().id());
            self.started.send(()).expect("test receiver dropped");
            self.release
                .lock()
                .expect("lock poisoned")
                .recv()
                .expect("test release sender dropped");
            Ok(VersionedPersistentState {
                revision: 0,
                state: PersistentState::default(),
            })
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn blocked_persistence_runs_off_runtime_thread_and_runtime_stays_responsive() {
        let runtime_thread = std::thread::current().id();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let persistence = Arc::new(ThreadRecordingPersistence {
            operation_thread: Mutex::new(None),
            started: started_tx,
            release: Mutex::new(release_rx),
        });
        let adapter = AsyncPersistence::new(persistence.clone());

        let load = tokio::spawn(async move { adapter.load().await });
        tokio::task::spawn_blocking(move || {
            started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("persistence did not start")
        })
        .await
        .expect("start waiter panicked");

        tokio::time::timeout(Duration::from_millis(100), tokio::task::yield_now())
            .await
            .expect("current-thread runtime was blocked");
        let operation_thread = persistence
            .operation_thread
            .lock()
            .expect("lock poisoned")
            .expect("operation thread not recorded");
        assert_ne!(operation_thread, runtime_thread);

        release_tx.send(()).expect("load task dropped");
        load.await.expect("load task panicked").unwrap();
    }

    #[tokio::test]
    async fn join_failure_is_reported_as_fail_closed_persistence_error() {
        let error = run_blocking_persistence::<(), _>("panic test", || {
            panic!("injected blocking task panic")
        })
        .await
        .expect_err("join failure must fail closed");

        assert!(matches!(error, ConxianError::Persistence(_)));
        assert!(error
            .to_string()
            .contains("blocking persistence task 'panic test' failed"));
    }
}
