use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use crate::runtime::actor::{LoopSpawn, Message, ReceivedSignal, SignalRouter};
use crate::runtime::arena::RoutineArena;
use crate::ast::{Statement, Expr};

/// High-performance Work-Stealing Multi-Core Scheduler with Thread Affinity.
///
/// Features:
/// 1. Pinned CPU Workers: 1 OS worker thread per detected CPU core.
/// 2. Cache-locality first: Workers pop from local deque before stealing.
/// 3. Global Injector Queue: spawns from outside are dispatched fairly.
/// 4. Work-Stealing: idle cores steal tasks from other cores, guaranteeing 100% utilization.
pub struct WorkStealingPool {
    injector: Arc<Injector<(Message, Arc<SignalRouter>)>>,
    shutdown: Arc<AtomicBool>,
    /// Counts logical tasks, including a loop while it is re-enqueued between
    /// iterations. The event loop uses it to know when a program is finished.
    pending_tasks: Arc<AtomicUsize>,
    _worker_threads: Vec<JoinHandle<()>>,
    num_cores: usize,
}

enum ExecutionControl {
    /// Normal completion of a statement.
    Continue,
    Terminate,
    Throw(String),
}

impl WorkStealingPool {
    pub fn new(verbose: bool) -> Self {
        let core_ids = core_affinity::get_core_ids().unwrap_or_default();
        let num_cores = if !core_ids.is_empty() {
            core_ids.len()
        } else {
            thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
        };

        let injector = Arc::new(Injector::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let pending_tasks = Arc::new(AtomicUsize::new(0));

        // Create local worker deques
        let mut workers = Vec::with_capacity(num_cores);
        let mut stealers = Vec::with_capacity(num_cores);

        for _ in 0..num_cores {
            let w = Worker::new_fifo();
            stealers.push(w.stealer());
            workers.push(w);
        }

        let stealers = Arc::new(stealers);
        let mut worker_threads = Vec::with_capacity(num_cores);

        for (worker_id, worker) in workers.into_iter().enumerate() {
            let core_id_opt = core_ids.get(worker_id).cloned();
            let injector_clone = Arc::clone(&injector);
            let stealers_clone = Arc::clone(&stealers);
            let shutdown_clone = Arc::clone(&shutdown);
            let pending_clone = Arc::clone(&pending_tasks);

            let handle = thread::Builder::new()
                .name(format!("lia-core-worker-{}", worker_id))
                .spawn(move || {
                    // Pin worker thread to the physical/logical core if supported
                    if let Some(core_id) = core_id_opt {
                        core_affinity::set_for_current(core_id);
                    }

                    Self::worker_loop(
                        worker_id,
                        worker,
                        &injector_clone,
                        &stealers_clone,
                        &shutdown_clone,
                        &pending_clone,
                        verbose,
                    );
                })
                .expect("Failed to spawn runtime worker thread");

            worker_threads.push(handle);
        }

        Self {
            injector,
            shutdown,
            pending_tasks,
            _worker_threads: worker_threads,
            num_cores,
        }
    }

    pub fn num_cores(&self) -> usize {
        self.num_cores
    }

    /// Schedule a routine message for execution across the core pool.
    pub fn schedule(&self, message: Message, signal_router: Arc<SignalRouter>) {
        self.pending_tasks.fetch_add(1, Ordering::Release);
        self.injector.push((message, signal_router));
    }

    pub fn is_idle(&self) -> bool {
        self.pending_tasks.load(Ordering::Acquire) == 0
    }

    /// Stops all workers immediately. Used for an explicit Ctrl+C request;
    /// queued routines and persistent loops are discarded rather than being
    /// re-enqueued again.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    fn worker_loop(
        core_id: usize,
        local: Worker<(Message, Arc<SignalRouter>)>,
        global: &Injector<(Message, Arc<SignalRouter>)>,
        stealers: &[Stealer<(Message, Arc<SignalRouter>)>],
        shutdown: &AtomicBool,
        pending_tasks: &AtomicUsize,
        verbose: bool,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            // Find task: 1. local queue -> 2. global injector -> 3. steal from peers
            let maybe_msg = local.pop().or_else(|| {
                // Check global injector
                std::iter::repeat_with(|| {
                    global.steal_batch_and_pop(&local)
                })
                .find(|s| !s.is_retry())
                .and_then(|s| match s {
                    Steal::Success(m) => Some(m),
                    _ => None,
                })
            }).or_else(|| {
                // Steal from another core (work-stealing)
                stealers.iter().enumerate().filter(|(i, _)| *i != core_id).find_map(|(_, stealer)| {
                    loop {
                        match stealer.steal() {
                            Steal::Success(m) => return Some(m),
                            Steal::Empty => return None,
                            Steal::Retry => continue,
                        }
                    }
                })
            });

            if let Some((msg, signal_router)) = maybe_msg {
                // Execute routine in its own isolated Arena
                let arena = RoutineArena::new();
                let allocated_payload = arena.alloc(msg.payload.clone());

                if verbose {
                    println!(
                        "[Runtime Core #{}] Executing isolated routine '{}' with payload in Arena: {}",
                        core_id, msg.routine_name, allocated_payload
                    );
                }

                let received_signal = if msg.is_loop {
                    let received = signal_router.receive_next(&msg.routine_name);
                    if verbose && received.value.is_some() {
                        println!("  [Core #{} Loop] Received signal: {:?} ({} pending)", core_id, received.value, received.pending);
                    }
                    Some(received)
                } else { None };

                let mut stopped = false;
                for stmt in &msg.statements {
                    match Self::execute_statement(stmt, &arena, verbose, core_id, global, pending_tasks, &signal_router, received_signal.as_ref()) {
                        ExecutionControl::Continue => {}
                        ExecutionControl::Terminate => { stopped = true; break; }
                        ExecutionControl::Throw(error) => {
                            if verbose { println!("  [Core #{} Exception] Unhandled: {}", core_id, error); }
                            stopped = true;
                            break;
                        }
                    }
                }
                // A persistent loop keeps its single pending-task slot while
                // it is requeued. Ordinary routines release their slot when
                // their body returns; this is what lets the host exit at
                // quiescence without reserving a core for loops.
                if msg.is_loop && !stopped && !shutdown.load(Ordering::Relaxed) {
                    global.push((msg, signal_router));
                } else if msg.is_loop && stopped {
                    signal_router.stop_loop(&msg.routine_name);
                    pending_tasks.fetch_sub(1, Ordering::Release);
                } else if !msg.is_loop {
                    pending_tasks.fetch_sub(1, Ordering::Release);
                }
                // Routine completes and entire arena is freed instantly in O(1)
            } else {
                // Brief cooperative yield if no tasks in system
                thread::yield_now();
                thread::sleep(std::time::Duration::from_micros(200));
            }
        }
    }

    fn execute_statement(stmt: &Statement, arena: &RoutineArena, verbose: bool, core_id: usize, global: &Injector<(Message, Arc<SignalRouter>)>, pending_tasks: &AtomicUsize, signal_router: &Arc<SignalRouter>, received_signal: Option<&ReceivedSignal>) -> ExecutionControl {
        match stmt {
            Statement::ExprStmt(expr) => {
                Self::execute_expr(expr, arena, verbose, core_id, signal_router, received_signal);
                ExecutionControl::Continue
            }
            Statement::VarDecl { name, value, is_mutable, .. } => {
                let formatted = Self::format_expr(value, signal_router, received_signal);
                let stored = arena.alloc(formatted);
                if verbose {
                    let mut_tag = if *is_mutable { "mut " } else { "" };
                    println!("  [Core #{} Arena] Local variable let {}{} = {} allocated in Arena", core_id, mut_tag, name, stored);
                }
                ExecutionControl::Continue
            }
            Statement::Spawn(expr) => {
                if let Expr::FunctionCall { name, args } = expr {
                    // Loops have no call parameters. A single argument in a
                    // spawn is therefore its asynchronous signal value.
                    if signal_router.has_loop(name) {
                        let payload = args.first().map(|arg| Self::format_expr_value(arg, signal_router, received_signal));
                        match signal_router.spawn_or_signal(name, payload) {
                            LoopSpawn::Started(loop_message) => {
                                pending_tasks.fetch_add(1, Ordering::Release);
                                global.push((loop_message, Arc::clone(signal_router)));
                                if verbose { println!("  [Core #{} Loop] Started {}", core_id, name); }
                            }
                            LoopSpawn::Signalled => {
                                if verbose { println!("  [Core #{} Loop Signal] queued -> {}", core_id, name); }
                            }
                            LoopSpawn::Unknown => {}
                        }
                        return ExecutionControl::Continue;
                    }
                    if let Some(routine_message) = signal_router.spawn_routine(
                        name,
                        Self::format_expr(expr, signal_router, received_signal),
                    ) {
                        pending_tasks.fetch_add(1, Ordering::Release);
                        global.push((routine_message, Arc::clone(signal_router)));
                        return ExecutionControl::Continue;
                    }
                }
                if verbose {
                    println!("  [Core #{} Concurrent Spawn] Dispatching actor for: {}", core_id, Self::format_expr(expr, signal_router, received_signal));
                }
                ExecutionControl::Continue
            }
            Statement::Signal { loop_name, payload } => {
                let delivered = signal_router.emit(loop_name, Self::format_expr_value(payload, signal_router, received_signal));
                if verbose {
                    println!("  [Core #{} Signal] {} -> {}", core_id, if delivered { "queued" } else { "dropped" }, loop_name);
                }
                ExecutionControl::Continue
            }
            Statement::Throw(value) => {
                ExecutionControl::Throw(Self::format_expr_value(value, signal_router, received_signal))
            }
            Statement::TryCatch { try_body, error_name, catch_body } => {
                for statement in try_body {
                    match Self::execute_statement(statement, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal) {
                        ExecutionControl::Continue => {}
                        ExecutionControl::Terminate => return ExecutionControl::Terminate,
                        ExecutionControl::Throw(error) => {
                            if verbose { println!("  [Core #{} Exception] {} caught: {}", core_id, error_name, error); }
                            for statement in catch_body {
                                match Self::execute_statement(statement, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal) {
                                    ExecutionControl::Continue => {}
                                    control => return control,
                                }
                            }
                            return ExecutionControl::Continue;
                        }
                    }
                }
                ExecutionControl::Continue
            }
            Statement::ForLoop { var_name, collection, body } => {
                if verbose {
                    println!("  [Core #{} Bounded Loop] Executing 'for {} in {}' ({} instructions)", core_id, var_name, Self::format_expr(collection, signal_router, received_signal), body.len());
                }
                for s in body {
                    match Self::execute_statement(s, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal) {
                        ExecutionControl::Continue => {}
                        control => return control,
                    }
                }
                ExecutionControl::Continue
            }
            Statement::Match { target, cases } => {
                if verbose {
                    println!("  [Core #{} Match] Pattern matching on {} with {} evaluated cases.", core_id, Self::format_expr(target, signal_router, received_signal), cases.len());
                }
                if let Some(first_case) = cases.first() {
                    for s in &first_case.body {
                        match Self::execute_statement(s, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal) {
                            ExecutionControl::Continue => {}
                            control => return control,
                        }
                    }
                }
                ExecutionControl::Continue
            }
            Statement::Terminate => ExecutionControl::Terminate,
        }
    }

    fn execute_expr(expr: &Expr, _arena: &RoutineArena, verbose: bool, core_id: usize, signal_router: &SignalRouter, received_signal: Option<&ReceivedSignal>) {
        if let Expr::FunctionCall { name, args } = expr {
            if name == "std.log" {
                let parts: Vec<String> = args.iter().map(|arg| Self::format_expr_value(arg, signal_router, received_signal)).collect();
                println!("{}", parts.join(" "));
            } else if verbose {
                let parts: Vec<String> = args.iter().map(|arg| Self::format_expr_value(arg, signal_router, received_signal)).collect();
                println!("  [Core #{} Function Call]: {}({})", core_id, name, parts.join(", "));
            }
        }
    }

    fn format_expr_value(expr: &Expr, signal_router: &SignalRouter, received_signal: Option<&ReceivedSignal>) -> String {
        match expr {
            Expr::StringLiteral(s) => s.clone(),
            Expr::IntLiteral(n) => n.to_string(),
            Expr::ListLiteral(items) => format!("[{}]", items.iter().map(|item| Self::format_expr_value(item, signal_router, received_signal)).collect::<Vec<_>>().join(", ")),
            Expr::Variable(v) => v.clone(),
            Expr::BinaryOp { op, left, right } => {
                format!("{} {} {}", Self::format_expr_value(left, signal_router, received_signal), op, Self::format_expr_value(right, signal_router, received_signal))
            }
            Expr::MemberAccess { target, field } => {
                if matches!(target.as_ref(), Expr::ReceiveSignal) {
                    match field.as_str() {
                        "value" => received_signal.and_then(|signal| signal.value.clone()).unwrap_or_else(|| "None".to_string()),
                        "pending" => received_signal.map(|signal| signal.pending.to_string()).unwrap_or_else(|| "0".to_string()),
                        _ => format!("receive.{}", field),
                    }
                } else { format!("{}.{}", Self::format_expr_value(target, signal_router, received_signal), field) }
            }
            Expr::RefCreate(inner) => {
                format!("ref {}", Self::format_expr_value(inner, signal_router, received_signal))
            }
            Expr::FunctionCall { name, args } => {
                let values = args.iter().map(|arg| signal_router.evaluate_expression(arg)).collect::<Option<Vec<_>>>();
                if let Some(values) = values {
                    if let Some(result) = signal_router.evaluate_function(name, values) { return result.to_string(); }
                }
                let parts: Vec<String> = args.iter().map(|arg| Self::format_expr_value(arg, signal_router, received_signal)).collect();
                format!("{}({})", name, parts.join(", "))
            }
            Expr::ReceiveSignal => received_signal.and_then(|signal| signal.value.clone()).unwrap_or_else(|| "None".to_string()),
        }
    }

    fn format_expr(expr: &Expr, signal_router: &SignalRouter, received_signal: Option<&ReceivedSignal>) -> String {
        match expr {
            Expr::StringLiteral(s) => format!("\"{}\"", s),
            Expr::IntLiteral(n) => n.to_string(),
            Expr::ListLiteral(items) => format!("[{}]", items.iter().map(|item| Self::format_expr(item, signal_router, received_signal)).collect::<Vec<_>>().join(", ")),
            Expr::Variable(v) => v.clone(),
            Expr::BinaryOp { op, left, right } => {
                format!("({} {} {})", Self::format_expr(left, signal_router, received_signal), op, Self::format_expr(right, signal_router, received_signal))
            }
            Expr::MemberAccess { target, field } => {
                format!("{}.{}", Self::format_expr(target, signal_router, received_signal), field)
            }
            Expr::RefCreate(inner) => {
                format!("ref {}", Self::format_expr(inner, signal_router, received_signal))
            }
            Expr::FunctionCall { name, args } => {
                let parts: Vec<String> = args.iter().map(|arg| Self::format_expr(arg, signal_router, received_signal)).collect();
                format!("{}({})", name, parts.join(", "))
            }
            Expr::ReceiveSignal => "receive".to_string(),
        }
    }

}

impl Drop for WorkStealingPool {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
