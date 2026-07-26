use std::time::Duration;
use tokio::sync::watch;

/// Return `true` when shutdown is already requested or arrives before the
/// next polling interval. Call this only between complete unit-of-work
/// futures so in-flight persistence operations are always joined.
pub(crate) async fn sleep_or_shutdown(
    shutdown: &mut watch::Receiver<bool>,
    interval: Duration,
) -> bool {
    if *shutdown.borrow() {
        return true;
    }

    tokio::select! {
        _ = tokio::time::sleep(interval) => false,
        changed = shutdown.changed() => changed.is_err() || *shutdown.borrow(),
    }
}
