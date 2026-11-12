use std::sync::Arc;
use tokio::sync::mpsc;
use crate::runtime::actor::{Message, SignalRouter};
use crate::runtime::work_stealing::WorkStealingPool;

pub struct RuntimeEventLoop {
    receiver: mpsc::Receiver<Message>,
    pool: Arc<WorkStealingPool>,
    verbose: bool,
    signal_router: Arc<SignalRouter>,
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
            signal_router: Arc::new(SignalRouter::default()),
        }
    }

    pub fn signal_router(&self) -> Arc<SignalRouter> {
        Arc::clone(&self.signal_router)
    }

    pub fn shutdown_handle(&self) -> Arc<WorkStealingPool> {
        Arc::clone(&self.pool)
    }

    pub async fn run(&mut self) {
        let cores = self.pool.num_cores();
        if self.verbose {
            println!(
                "[Runtime EventLoop] Work-stealing scheduler active with {} dedicated CPU worker cores (Thread Affinity enabled).",
                cores
            );
        }

        // External submissions arrive through Tokio. Work spawned by a worker
        // is placed directly in the shared injector to avoid blocking it on
        // this receiver.
        while let Some(msg) = self.receiver.recv().await {
            if msg.is_loop && !self.signal_router.start_loop(&msg.routine_name) {
                if self.verbose {
                    println!("[Runtime EventLoop] Ignoring duplicate start of loop '{}'.", msg.routine_name);
                }
                continue;
            }
            // Dispatches to the multi-core work-stealing pool
            self.pool.schedule(msg, Arc::clone(&self.signal_router));
        }

        // The input channel is closed after `main` is dispatched. Internal
        // spawns go directly to the shared pool, so wait until every routine
        // and every active loop has completed before ending the program.
        while !self.pool.is_idle() && !self.pool.is_shutdown() {
            tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        }
    }
}

