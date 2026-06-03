//! Session trait - unified training abstraction
use std::sync::Arc;
use tokio_postgres::Client;

/// Unified training session interface.
/// Both fast and slow modes implement this for polymorphic training loops.
#[async_trait::async_trait]
pub trait Trainer: Send + Sync + Sized {
    /// Database client for persistence operations.
    fn client(&self) -> &Arc<Client>;
    /// Sync in-memory state to database on graceful exit.
    async fn sync(self);
    /// Run one training iteration.
    async fn step(&mut self);
    /// Get current epoch count.
    async fn epoch(&self) -> usize;
    /// Get final summary on completion.
    async fn summary(&self) -> String;
    /// Get training statistics if checkpoint interval has elapsed.
    async fn checkpoint(&self) -> Option<String>;

    async fn train(mut self) {
        log::info!("training blueprint");
        // The `RBP_FAST_EPOCHS` smoke knob caps the loop to a fixed
        // number of steps. When unset (the production default), the
        // loop runs until the graceful `interrupted()` signal fires
        // (Ctrl+C, `Q` from stdin, or `TRAIN_DURATION` deadline).
        let budget = rbp_core::fast_epochs();
        if let Some(n) = budget {
            log::info!("smoke budget: {n} epoch(s) (RBP_FAST_EPOCHS)");
        } else {
            log::info!("press 'Q + ↵' to stop gracefully");
        }
        let mut taken: usize = 0;
        loop {
            if let Some(n) = budget
                && taken >= n
            {
                log::info!("smoke budget exhausted after {taken} epoch(s)");
                break;
            }
            self.step().await;
            taken += 1;
            self.checkpoint().await.map(|s| log::info!("{}", s));
            if rbp_core::interrupted() {
                log::info!("{}", self.summary().await);
                break;
            }
        }
        self.sync().await;
    }
}
