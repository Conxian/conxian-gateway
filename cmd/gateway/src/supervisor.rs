use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::{Id, JoinSet};
use tracing::{error, info, warn};

type CriticalTaskStarter = Box<
    dyn FnOnce(watch::Receiver<bool>) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + 'static,
>;

/// A process-critical, long-lived task supervised with the HTTP server.
pub struct CriticalTask {
    name: &'static str,
    start: CriticalTaskStarter,
}

impl CriticalTask {
    pub fn new<F, Fut>(name: &'static str, start: F) -> Self
    where
        F: FnOnce(watch::Receiver<bool>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        Self {
            name,
            start: Box::new(move |shutdown| Box::pin(start(shutdown))),
        }
    }
}

struct NamedTaskExit {
    name: &'static str,
    result: anyhow::Result<()>,
    cancellation_observed: bool,
}

enum ShutdownTrigger {
    Signal,
    Fatal(String),
}

/// Wait until coordinated shutdown is requested. A closed sender is also a
/// shutdown request, preventing a task from waiting forever during teardown.
pub async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            return;
        }
    }
}

/// Supervise every process-critical task as one failure domain.
///
/// Any error, panic, or unexpected normal return starts coordinated shutdown.
/// Signal shutdown is successful only when all tasks join within `grace` and
/// none reports an error or panic. Tasks still alive at the deadline are
/// aborted and the process returns failure.
pub async fn supervise<S>(
    tasks: Vec<CriticalTask>,
    shutdown_signal: S,
    grace: Duration,
) -> anyhow::Result<()>
where
    S: Future<Output = anyhow::Result<()>> + Send,
{
    if tasks.is_empty() {
        anyhow::bail!("critical task supervisor requires at least one task");
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut running_names = BTreeSet::new();
    let mut task_names = HashMap::<Id, &'static str>::new();
    let mut join_set = JoinSet::new();
    for task in tasks {
        running_names.insert(task.name);
        let future = (task.start)(shutdown_rx.clone());
        let cancellation_observer = shutdown_rx.clone();
        let abort_handle = join_set.spawn(async move {
            let result = future.await;
            NamedTaskExit {
                name: task.name,
                result,
                cancellation_observed: *cancellation_observer.borrow(),
            }
        });
        task_names.insert(abort_handle.id(), task.name);
    }
    drop(shutdown_rx);

    tokio::pin!(shutdown_signal);
    let trigger = tokio::select! {
        biased;
        signal = &mut shutdown_signal => {
            match signal {
                Ok(()) => ShutdownTrigger::Signal,
                Err(error) => ShutdownTrigger::Fatal(format!("shutdown signal monitor failed: {error:#}")),
            }
        }
        joined = join_set.join_next() => {
            let joined = joined.expect("non-empty critical task set must produce an exit");
            match joined {
                Ok(exit) => {
                    running_names.remove(exit.name);
                    ShutdownTrigger::Fatal(
                        exit_failure_message(&exit)
                            .expect("an exit before supervisor cancellation must be fatal"),
                    )
                }
                Err(error) => {
                    let name = task_names.get(&error.id()).copied().unwrap_or("unknown");
                    running_names.remove(name);
                    ShutdownTrigger::Fatal(format!("critical task '{name}' join failed: {error}"))
                }
            }
        }
    };

    match &trigger {
        ShutdownTrigger::Signal => info!("Shutdown signal received; stopping critical tasks"),
        ShutdownTrigger::Fatal(message) => {
            error!(reason = %message, "Critical task failure; stopping Gateway")
        }
    }
    let _ = shutdown_tx.send(true);

    let deadline = tokio::time::sleep(grace);
    tokio::pin!(deadline);
    let mut shutdown_failures = Vec::new();
    let mut timed_out = false;
    while !join_set.is_empty() {
        tokio::select! {
            joined = join_set.join_next() => {
                match joined.expect("join set reported non-empty") {
                    Ok(exit) => {
                        running_names.remove(exit.name);
                        match exit_failure_message(&exit) {
                            None => {
                                info!(task = exit.name, "Critical task stopped");
                            }
                            Some(message) => {
                                error!(reason = %message);
                                shutdown_failures.push(message);
                            }
                        }
                    }
                    Err(error) if error.is_cancelled() && timed_out => {}
                    Err(error) => {
                        let name = task_names.get(&error.id()).copied().unwrap_or("unknown");
                        running_names.remove(name);
                        shutdown_failures.push(format!("critical task '{name}' join failed during shutdown: {error}"));
                    }
                }
            }
            _ = &mut deadline, if !timed_out => {
                timed_out = true;
                warn!(
                    grace_ms = grace.as_millis(),
                    tasks = ?running_names,
                    "Critical task shutdown grace expired; aborting remaining tasks"
                );
                join_set.abort_all();
            }
        }
    }

    if timed_out {
        shutdown_failures.push(format!(
            "critical task shutdown exceeded {} ms; aborted: {}",
            grace.as_millis(),
            running_names.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }

    match trigger {
        ShutdownTrigger::Fatal(message) => {
            if shutdown_failures.is_empty() {
                anyhow::bail!(message);
            }
            anyhow::bail!("{message}; {}", shutdown_failures.join("; "));
        }
        ShutdownTrigger::Signal if !shutdown_failures.is_empty() => {
            anyhow::bail!(shutdown_failures.join("; "));
        }
        ShutdownTrigger::Signal => Ok(()),
    }
}

fn exit_failure_message(exit: &NamedTaskExit) -> Option<String> {
    match &exit.result {
        Ok(()) if exit.cancellation_observed => None,
        Ok(()) => Some(format!(
            "critical task '{}' returned unexpectedly before observing a shutdown request",
            exit.name
        )),
        Err(error) => Some(format!("critical task '{}' failed: {error:#}", exit.name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use conxian_core::{
        BlockInfo, ConxianError, ConxianResult, GatewayState, Persistence, PersistentState,
        VersionedPersistentState,
    };
    use conxian_engine::bitcoin::{BitcoinListener, BitcoinRpc};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, RwLock,
    };

    const TEST_GRACE: Duration = Duration::from_millis(100);

    fn pending_signal() -> impl Future<Output = anyhow::Result<()>> + Send {
        std::future::pending()
    }

    struct TestBitcoinRpc;

    #[async_trait]
    impl BitcoinRpc for TestBitcoinRpc {
        async fn get_block_count(&self) -> ConxianResult<u64> {
            Ok(1)
        }

        async fn get_block_info(&self, height: u64) -> ConxianResult<BlockInfo> {
            Ok(BlockInfo {
                hash: "supervisor-test-block".to_string(),
                height,
                timestamp: 1,
            })
        }

        async fn get_network_info(&self) -> ConxianResult<String> {
            Ok("regtest".to_string())
        }
    }

    struct FailingPersistence;

    impl Persistence for FailingPersistence {
        fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
            Ok(VersionedPersistentState {
                revision: 0,
                state: PersistentState::default(),
            })
        }

        fn compare_and_swap(
            &self,
            _expected_revision: u64,
            _new_state: &PersistentState,
        ) -> ConxianResult<VersionedPersistentState> {
            Err(ConxianError::Persistence(
                "injected durable checkpoint failure".to_string(),
            ))
        }
    }

    struct BlockingPersistence {
        state: Mutex<VersionedPersistentState>,
        started: mpsc::Sender<()>,
        completed: mpsc::Sender<()>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl Persistence for BlockingPersistence {
        fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
            Ok(self.state.lock().expect("lock poisoned").clone())
        }

        fn compare_and_swap(
            &self,
            expected_revision: u64,
            new_state: &PersistentState,
        ) -> ConxianResult<VersionedPersistentState> {
            self.started
                .send(())
                .expect("persistence start receiver dropped");
            self.release
                .lock()
                .expect("lock poisoned")
                .recv()
                .expect("persistence release sender dropped");
            let mut current = self.state.lock().expect("lock poisoned");
            if current.revision != expected_revision {
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            current.revision += 1;
            current.state = new_state.clone();
            let updated = current.clone();
            self.completed
                .send(())
                .expect("persistence completion receiver dropped");
            Ok(updated)
        }
    }

    struct ReleaseOnDrop(Option<mpsc::Sender<()>>);

    impl ReleaseOnDrop {
        fn release(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            self.release();
        }
    }

    async fn bitcoin_listener_task(
        persistence: Arc<dyn Persistence>,
    ) -> (CriticalTask, Arc<AtomicBool>) {
        let state = Arc::new(RwLock::new(GatewayState::default()));
        let mut listener = BitcoinListener::new(TestBitcoinRpc, state, persistence, None, 0)
            .await
            .unwrap();
        let peer_stopped = Arc::new(AtomicBool::new(false));
        let listener_task = CriticalTask::new("Bitcoin listener", move |shutdown| async move {
            listener
                .run_until_shutdown(shutdown)
                .await
                .map_err(Into::into)
        });
        (listener_task, peer_stopped)
    }

    #[tokio::test]
    async fn fatal_worker_error_cancels_and_joins_peers_and_http_equivalent() {
        let peer_stopped = Arc::new(AtomicBool::new(false));
        let http_stopped = Arc::new(AtomicBool::new(false));
        let tasks = vec![
            CriticalTask::new("durable writer", |_shutdown| async {
                Err(anyhow::anyhow!("disk failed"))
            }),
            stopping_task("peer", Arc::clone(&peer_stopped)),
            stopping_task("http", Arc::clone(&http_stopped)),
        ];

        let error = supervise(tasks, pending_signal(), TEST_GRACE)
            .await
            .expect_err("worker failure must be process-fatal");
        assert!(error.to_string().contains("durable writer"));
        assert!(error.to_string().contains("disk failed"));
        assert!(peer_stopped.load(Ordering::SeqCst));
        assert!(http_stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn listener_durability_failure_stops_http_equivalent_peer() {
        let (listener, http_stopped) = bitcoin_listener_task(Arc::new(FailingPersistence)).await;
        let tasks = vec![listener, stopping_task("http", Arc::clone(&http_stopped))];

        let error = supervise(tasks, pending_signal(), TEST_GRACE)
            .await
            .expect_err("listener durability failure must stop the process");
        assert!(error.to_string().contains("Bitcoin listener"));
        assert!(error
            .to_string()
            .contains("injected durable checkpoint failure"));
        assert!(http_stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn signal_waits_for_in_flight_listener_persistence_before_success() {
        let (started_tx, started_rx) = mpsc::channel();
        let (completed_tx, _completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut release = ReleaseOnDrop(Some(release_tx));
        let persistence = Arc::new(BlockingPersistence {
            state: Mutex::new(VersionedPersistentState {
                revision: 0,
                state: PersistentState::default(),
            }),
            started: started_tx,
            completed: completed_tx,
            release: Mutex::new(release_rx),
        });
        let (listener, http_stopped) = bitcoin_listener_task(persistence).await;
        let tasks = vec![listener, stopping_task("http", Arc::clone(&http_stopped))];
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel();
        let supervision = tokio::spawn(async move {
            supervise(
                tasks,
                async move { signal_rx.await.map_err(Into::into) },
                Duration::from_secs(1),
            )
            .await
        });
        tokio::task::spawn_blocking(move || {
            started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("listener persistence did not start")
        })
        .await
        .unwrap();

        signal_tx.send(()).unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            !supervision.is_finished(),
            "supervision reported success before persistence drained"
        );
        release.release();
        supervision.await.unwrap().unwrap();
        assert!(http_stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn in_flight_persistence_exceeding_grace_is_process_fatal() {
        let (started_tx, started_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut release = ReleaseOnDrop(Some(release_tx));
        let persistence = Arc::new(BlockingPersistence {
            state: Mutex::new(VersionedPersistentState {
                revision: 0,
                state: PersistentState::default(),
            }),
            started: started_tx,
            completed: completed_tx,
            release: Mutex::new(release_rx),
        });
        let (listener, http_stopped) = bitcoin_listener_task(persistence).await;
        let tasks = vec![listener, stopping_task("http", Arc::clone(&http_stopped))];
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel();
        let supervision = tokio::spawn(async move {
            supervise(
                tasks,
                async move { signal_rx.await.map_err(Into::into) },
                Duration::from_millis(20),
            )
            .await
        });
        tokio::task::spawn_blocking(move || {
            started_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("listener persistence did not start")
        })
        .await
        .unwrap();

        signal_tx.send(()).unwrap();
        let error = supervision
            .await
            .unwrap()
            .expect_err("hard shutdown timeout must never report clean shutdown");
        assert!(error.to_string().contains("exceeded 20 ms"));
        assert!(error.to_string().contains("Bitcoin listener"));
        assert!(http_stopped.load(Ordering::SeqCst));
        release.release();
        tokio::task::spawn_blocking(move || {
            completed_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("blocking persistence closure did not finish after release")
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn panic_is_fatal_and_visible() {
        let tasks = vec![
            CriticalTask::new("panicking writer", |_shutdown| async {
                panic!("writer panic evidence");
            }),
            stopping_task("http", Arc::new(AtomicBool::new(false))),
        ];

        let error = supervise(tasks, pending_signal(), TEST_GRACE)
            .await
            .expect_err("panic must be process-fatal");
        assert!(error.to_string().contains("panicking writer"));
        assert!(error.to_string().contains("writer panic evidence"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pre_cancellation_normal_return_is_fatal_when_signal_is_also_ready() {
        let returned = Arc::new(AtomicBool::new(false));
        let signal_ready = Arc::new(tokio::sync::Notify::new());
        let task_returned = Arc::clone(&returned);
        let task_signal_ready = Arc::clone(&signal_ready);
        let tasks = vec![CriticalTask::new("early return", |_shutdown| async move {
            task_returned.store(true, Ordering::SeqCst);
            task_signal_ready.notify_one();
            Ok(())
        })];

        let error = supervise(
            tasks,
            async move {
                signal_ready.notified().await;
                assert!(returned.load(Ordering::SeqCst));
                Ok(())
            },
            TEST_GRACE,
        )
        .await
        .expect_err("return before cancellation publication must remain fatal");

        assert!(error.to_string().contains("early return"));
        assert!(error
            .to_string()
            .contains("before observing a shutdown request"));
    }

    #[tokio::test]
    async fn normal_return_after_observing_signal_cancellation_is_clean() {
        let stopped = Arc::new(AtomicBool::new(false));
        let tasks = vec![stopping_task("worker", Arc::clone(&stopped))];

        supervise(tasks, async { Ok(()) }, TEST_GRACE)
            .await
            .expect("post-cancellation normal return should be clean");
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn signal_cancellation_joins_all_tasks() {
        let first_stopped = Arc::new(AtomicBool::new(false));
        let second_stopped = Arc::new(AtomicBool::new(false));
        let tasks = vec![
            stopping_task("worker", Arc::clone(&first_stopped)),
            stopping_task("http", Arc::clone(&second_stopped)),
        ];

        supervise(tasks, async { Ok(()) }, TEST_GRACE)
            .await
            .expect("signal shutdown should be clean");
        assert!(first_stopped.load(Ordering::SeqCst));
        assert!(second_stopped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn stuck_task_is_aborted_after_grace_timeout() {
        let tasks = vec![
            CriticalTask::new("stuck", |_shutdown| async {
                std::future::pending::<()>().await;
                Ok(())
            }),
            stopping_task("http", Arc::new(AtomicBool::new(false))),
        ];

        let started = tokio::time::Instant::now();
        let error = supervise(tasks, async { Ok(()) }, Duration::from_millis(20))
            .await
            .expect_err("stuck task must make shutdown fail");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(error.to_string().contains("stuck"));
        assert!(error.to_string().contains("aborted"));
    }

    fn stopping_task(name: &'static str, stopped: Arc<AtomicBool>) -> CriticalTask {
        CriticalTask::new(name, move |mut shutdown| async move {
            shutdown_requested(&mut shutdown).await;
            stopped.store(true, Ordering::SeqCst);
            Ok(())
        })
    }
}
