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

    /// Wrap a worker which has no native cancellation input. Cancellation
    /// drops its run future; blocking filesystem work remains isolated on
    /// Tokio's blocking pool and is not performed while async locks are held.
    pub fn until_shutdown<Fut, E>(name: &'static str, future: Fut) -> Self
    where
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: Into<anyhow::Error> + Send + 'static,
    {
        Self::new(name, move |mut shutdown| async move {
            tokio::select! {
                result = future => result.map_err(Into::into),
                _ = shutdown_requested(&mut shutdown) => Ok(()),
            }
        })
    }
}

struct NamedTaskExit {
    name: &'static str,
    result: anyhow::Result<()>,
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
        let abort_handle = join_set.spawn(async move {
            NamedTaskExit {
                name: task.name,
                result: future.await,
            }
        });
        task_names.insert(abort_handle.id(), task.name);
    }
    drop(shutdown_rx);

    tokio::pin!(shutdown_signal);
    let trigger = tokio::select! {
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
                    ShutdownTrigger::Fatal(unexpected_exit_message(&exit))
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
                        match exit.result {
                            Ok(()) => {
                                info!(task = exit.name, "Critical task stopped");
                            }
                            Err(error) => {
                                let message = format!("critical task '{}' failed during shutdown: {error:#}", exit.name);
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

fn unexpected_exit_message(exit: &NamedTaskExit) -> String {
    match &exit.result {
        Ok(()) => format!(
            "critical task '{}' returned unexpectedly without a shutdown request",
            exit.name
        ),
        Err(error) => {
            format!("critical task '{}' failed: {error:#}", exit.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    const TEST_GRACE: Duration = Duration::from_millis(100);

    fn pending_signal() -> impl Future<Output = anyhow::Result<()>> + Send {
        std::future::pending()
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
