//! Keeps the long-running network tasks alive for the whole session.
//!
//! Every listener and the reporting loop run as their own Tokio task, and a
//! Tokio task that panics or returns simply ends: nothing awaits its
//! handle, so the app carried on with frozen decks and no trace of why
//! until it was restarted. [`supervise`] runs such a task again after a
//! pause, logging why it ended.

use std::future::Future;
use std::time::Duration;

use tokio::task::JoinHandle;

/// Pause before running a task that ended again — long enough that a task
/// failing straight away (a port still held by another app) does not spin,
/// short enough that the decks come back within a few status packets.
pub const TASK_RESTART_DELAY: Duration = Duration::from_secs(2);

/// Runs the task `start` builds, and runs it again `restart_delay` after it
/// ends, for as long as this future is polled. Dropping this future — as
/// aborting the task it runs in does — aborts the supervised task with it.
pub async fn supervise<F, Fut>(task: &'static str, restart_delay: Duration, start: F)
where
    F: Fn() -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    loop {
        let mut running = AbortOnDrop(tokio::spawn(start()));
        match (&mut running.0).await {
            Ok(()) => tracing::warn!(task, "network task ended; restarting it"),
            Err(error) if error.is_panic() => {
                tracing::error!(task, %error, "network task panicked; restarting it")
            }
            Err(_) => return, // Cancelled from outside: the engine is stopping.
        }
        tokio::time::sleep(restart_delay).await;
    }
}

/// Aborts the task it holds when dropped, so the supervised task never
/// outlives its supervisor.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    const TEST_RESTART_DELAY: Duration = Duration::from_millis(5);

    /// The silent-death regression: a task that panics is run again
    /// instead of leaving its decks frozen for the rest of the session.
    #[tokio::test]
    async fn a_panicking_task_is_run_again() {
        let starts = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&starts);
        let supervisor = tokio::spawn(supervise("test", TEST_RESTART_DELAY, move || {
            let counted = Arc::clone(&counted);
            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                panic!("simulated failure");
            }
        }));

        tokio::time::timeout(Duration::from_secs(5), async {
            while starts.load(Ordering::SeqCst) < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the task should have been started again");
        supervisor.abort();
    }

    #[tokio::test]
    async fn stopping_the_supervisor_stops_the_task() {
        struct SetOnDrop(Arc<AtomicBool>);
        impl Drop for SetOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&dropped);
        let supervisor = tokio::spawn(supervise("test", TEST_RESTART_DELAY, move || {
            let guard = SetOnDrop(Arc::clone(&flag));
            async move {
                let _guard = guard;
                std::future::pending::<()>().await;
            }
        }));
        tokio::task::yield_now().await;

        supervisor.abort();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !dropped.load(Ordering::SeqCst) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the supervised task should have been aborted");
    }
}
