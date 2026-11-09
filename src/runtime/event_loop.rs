use std::sync::Arc;
use tokio::sync::mpsc;
use crate::runtime::actor::Message;
use crate::runtime::work_stealing::WorkStealingPool;

pub struct RuntimeEventLoop {
    receiver: mpsc::Receiver<Message>,
    pool: Arc<WorkStealingPool>,
    verbose: bool,
}

impl RuntimeEventLoop {
    pub fn new(receiver: mpsc::Receiver<Message>) -> Self {
        Self::with_verbose(receiver, false)
    }

    pub fn with_verbose(receiver: mpsc::Receiver<Message>, verbose: bool) -> Self {
        let pool = Arc::new(WorkStealingPool::new(verbose));
        Self {
            receiver,
            pool,
            verbose,
        }
    }

    pub async fn run(&mut self) {
        let cores = self.pool.num_cores();
        if self.verbose {
            println!(
                "[Runtime EventLoop] Work-stealing scheduler active with {} dedicated CPU worker cores (Thread Affinity enabled).",
                cores
            );
        }

        while let Some(msg) = self.receiver.recv().await {
            // Dispatches to the multi-core work-stealing pool
            self.pool.schedule(msg);
        }
    }
}

