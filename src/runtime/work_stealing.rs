use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use std::thread::{self, JoinHandle};
use crossbeam_deque::{Injector, Steal, Stealer, Worker};
use crate::runtime::actor::{LoopSpawn, Message, ReceivedSignal, RuntimeValue, SignalRouter, EvaluationFailure};
use crate::runtime::arena::RoutineArena;
use crate::ast::{Statement, Expr, TypeSpec};

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
    Failure(EvaluationFailure),
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

                let mut variable_types = HashMap::new();
                let mut variable_values = msg.bindings.clone();
                let mut stopped = false;
                for stmt in &msg.statements {
                    match Self::execute_statement(stmt, &arena, verbose, core_id, global, pending_tasks, &signal_router, received_signal.as_ref(), msg.is_loop.then_some(msg.routine_name.as_str()), &mut variable_types, &mut variable_values) {
                        ExecutionControl::Continue => {}
                        ExecutionControl::Terminate => { stopped = true; break; }
                        ExecutionControl::Failure(error) => {
                            signal_router.report_error(format!("routine '{}': {}", msg.routine_name, error));
                            shutdown.store(true, Ordering::Release);
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

    fn execute_statement(stmt: &Statement, arena: &RoutineArena, verbose: bool, core_id: usize, global: &Injector<(Message, Arc<SignalRouter>)>, pending_tasks: &AtomicUsize, signal_router: &Arc<SignalRouter>, received_signal: Option<&ReceivedSignal>, loop_name: Option<&str>, variable_types: &mut HashMap<String, String>, variable_values: &mut HashMap<String, RuntimeValue>) -> ExecutionControl {
        let evaluate = |expr: &Expr| signal_router.evaluate_expression_with_context(expr, variable_values, received_signal, loop_name, variable_types);
        macro_rules! value {
            ($expr:expr) => { match evaluate($expr) { Ok(value) => value, Err(error) => return ExecutionControl::Failure(error) } };
        }
        match stmt {
            Statement::ExprStmt(expr) => { value!(expr); ExecutionControl::Continue }
            Statement::VarDecl { name, value, is_mutable, type_annot } => {
                let result = value!(value);
                let type_name = type_annot.as_ref().map(Self::type_spec_name).unwrap_or_else(|| result.type_name());
                variable_types.insert(name.clone(), type_name);
                if verbose {
                    let stored = arena.alloc(result.to_string());
                    println!("  [Core #{} Arena] Local variable let {}{} = {} allocated in Arena", core_id, if *is_mutable { "mut " } else { "" }, name, stored);
                }
                variable_values.insert(name.clone(), result);
                ExecutionControl::Continue
            }
            Statement::MemorySet { name, value } => {
                let result = value!(value);
                let Some(loop_name) = loop_name else { return ExecutionControl::Failure(EvaluationFailure::Invalid); };
                signal_router.memory_set(loop_name, name.clone(), result.to_string());
                ExecutionControl::Continue
            }
            Statement::Spawn(expr) => {
                let Some((name, args)) = Self::spawn_call(expr) else { return ExecutionControl::Failure(EvaluationFailure::Invalid); };
                if variable_values.contains_key(&name) || name.split_once('.').is_some_and(|(root, _)| variable_values.contains_key(root)) {
                    return ExecutionControl::Failure(EvaluationFailure::Invalid);
                }
                let mut values = Vec::with_capacity(args.len());
                for arg in args { values.push(value!(arg)); }
                if signal_router.has_loop(&name) {
                    if values.len() > 1 { return ExecutionControl::Failure(EvaluationFailure::Invalid); }
                    match signal_router.spawn_or_signal(&name, values.first().map(ToString::to_string)) {
                        LoopSpawn::Started(message) => {
                            pending_tasks.fetch_add(1, Ordering::Release);
                            global.push((message, Arc::clone(signal_router)));
                        }
                        LoopSpawn::Signalled => {}
                        LoopSpawn::Unknown => return ExecutionControl::Failure(EvaluationFailure::Invalid),
                    }
                } else {
                    let Some(message) = signal_router.spawn_routine_with_args(&name, format!("Spawn {}", name), values) else {
                        return ExecutionControl::Failure(EvaluationFailure::Invalid);
                    };
                    pending_tasks.fetch_add(1, Ordering::Release);
                    global.push((message, Arc::clone(signal_router)));
                }
                ExecutionControl::Continue
            }
            Statement::Signal { loop_name: target, payload } => {
                let payload = value!(payload).to_string();
                let delivered = signal_router.emit(target, payload);
                if verbose { println!("  [Core #{} Signal] {} -> {}", core_id, if delivered { "queued" } else { "dropped" }, target); }
                ExecutionControl::Continue
            }
            Statement::Throw(expr) => ExecutionControl::Failure(EvaluationFailure::Thrown(value!(expr))),
            Statement::TryCatch { try_body, error_name, catch_body } => {
                let mut local_values = variable_values.clone();
                let mut local_types = variable_types.clone();
                for stmt in try_body {
                    match Self::execute_statement(stmt, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal, loop_name, &mut local_types, &mut local_values) {
                        ExecutionControl::Continue => {}
                        ExecutionControl::Failure(EvaluationFailure::Thrown(error)) => {
                            let mut catch_values = variable_values.clone();
                            let mut catch_types = variable_types.clone();
                            catch_types.insert(error_name.clone(), error.type_name());
                            catch_values.insert(error_name.clone(), error);
                            for stmt in catch_body {
                                match Self::execute_statement(stmt, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal, loop_name, &mut catch_types, &mut catch_values) {
                                    ExecutionControl::Continue => {}
                                    control => return control,
                                }
                            }
                            return ExecutionControl::Continue;
                        }
                        control => return control,
                    }
                }
                ExecutionControl::Continue
            }
            Statement::ForLoop { var_name, collection, body } => {
                let RuntimeValue::List(items) = value!(collection) else { return ExecutionControl::Failure(EvaluationFailure::Invalid); };
                for item in items {
                    let mut local_values = variable_values.clone();
                    let mut local_types = variable_types.clone();
                    local_types.insert(var_name.clone(), item.type_name());
                    local_values.insert(var_name.clone(), item);
                    for stmt in body {
                        match Self::execute_statement(stmt, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal, loop_name, &mut local_types, &mut local_values) {
                            ExecutionControl::Continue => {}
                            control => return control,
                        }
                    }
                }
                ExecutionControl::Continue
            }
            Statement::Match { target, cases } => {
                let target = value!(target);
                let Some(case) = cases.iter().find(|case| SignalRouter::pattern_matches(&case.pattern, &target)) else { return ExecutionControl::Failure(EvaluationFailure::Invalid); };
                let mut local_values = variable_values.clone();
                let mut local_types = variable_types.clone();
                SignalRouter::bind_pattern(&case.pattern, &target, &mut local_values);
                for (name, value) in &local_values { local_types.entry(name.clone()).or_insert_with(|| value.type_name()); }
                for stmt in &case.body {
                    match Self::execute_statement(stmt, arena, verbose, core_id, global, pending_tasks, signal_router, received_signal, loop_name, &mut local_types, &mut local_values) {
                        ExecutionControl::Continue => {}
                        control => return control,
                    }
                }
                ExecutionControl::Continue
            }
            Statement::Terminate => ExecutionControl::Terminate,
        }
    }

    fn spawn_call(expr: &Expr) -> Option<(String, &[Expr])> {
        fn target_name(target: &Expr) -> Option<String> {
            match target {
                Expr::Variable(name) => Some(name.clone()),
                Expr::MemberAccess { target, field } => Some(format!("{}.{}", target_name(target)?, field)),
                Expr::Specialize { target, .. } => target_name(target),
                _ => None,
            }
        }
        match expr {
            Expr::FunctionCall { name, args } => Some((name.clone(), args)),
            Expr::IndirectCall { target, args } => Some((target_name(target)?, args)),
            _ => None,
        }
    }

    fn type_spec_name(spec: &TypeSpec) -> String {
        match spec {
            TypeSpec::Primitive(name) => name.clone(),
            TypeSpec::Ref(inner) => format!("Ref[{}]", Self::type_spec_name(inner)),
            TypeSpec::Function(args, result) => format!("({}) -> {}", args.iter().map(Self::type_spec_name).collect::<Vec<_>>().join(", "), Self::type_spec_name(result)),
            TypeSpec::Generic(name, args) => format!("{}[{}]", name, args.iter().map(Self::type_spec_name).collect::<Vec<_>>().join(", ")),
        }
    }


}

impl Drop for WorkStealingPool {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
