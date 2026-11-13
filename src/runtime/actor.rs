use std::collections::HashMap;
use std::sync::Mutex;
use crossbeam_channel::{Receiver, Sender};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use crate::ast::{Expr, FuncDecl, Param, Pattern, Statement};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Name used for diagnostics and loop-mailbox lookup.
    pub routine_name: String,
    pub payload: String,
    #[serde(default)]
    pub statements: Vec<Statement>,
    #[serde(default)]
    pub is_loop: bool,
    #[serde(default)]
    pub bindings: HashMap<String, RuntimeValue>,
}

pub struct ActorHandle {
    sender: mpsc::Sender<Message>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationFailure {
    Invalid,
    Thrown(RuntimeValue),
}

/// Mailboxes for persistent loops. `Sender::send` on an unbounded channel
/// never waits for the receiving loop, so emitting a signal cannot block its
/// caller.
#[derive(Default)]
pub struct SignalRouter {
    /// One FIFO mailbox plus lifecycle state for each declared loop.
    mailboxes: Mutex<HashMap<String, LoopMailbox>>,
    /// Private persistent cells keyed by loop declaration name.
    loop_memory: Mutex<HashMap<String, HashMap<String, String>>>,
    /// Routine bodies are registered once at startup so `spawn Worker()` can
    /// materialize an independent task without sharing local arena state.
    routines: Mutex<HashMap<String, (Vec<Param>, Vec<Statement>)>>,
    /// User functions are interpreted from their AST. Runtime intrinsics are
    /// deliberately limited to the explicitly imported `std.*` namespace.
    functions: Mutex<HashMap<String, FuncDecl>>,
    errors: Mutex<Vec<String>>,
}

struct LoopMailbox {
    sender: Sender<String>,
    receiver: Receiver<String>,
    active: bool,
    statements: Vec<Statement>,
}

#[derive(Debug)]
pub enum LoopSpawn {
    Started(Message),
    Signalled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedSignal {
    pub value: Option<String>,
    /// Number of items left after `value` was removed.
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeValue {
    Int(i64),
    String(String),
    Bool(bool),
    List(Vec<RuntimeValue>),
    /// A named function without captured state. Cloning copies its identity.
    Function(String),
    None,
}

impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeValue::Int(value) => write!(formatter, "{}", value),
            RuntimeValue::String(value) => write!(formatter, "{}", value),
            RuntimeValue::Bool(value) => write!(formatter, "{}", value),
            RuntimeValue::Function(name) => write!(formatter, "{}", name),
            RuntimeValue::List(items) => {
                write!(formatter, "[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 { write!(formatter, ", ")?; }
                    write!(formatter, "{}", item)?;
                }
                write!(formatter, "]")
            }
            RuntimeValue::None => write!(formatter, "None"),
        }
    }
}

impl RuntimeValue {
    pub(crate) fn type_name(&self) -> String {
        match self {
            Self::Int(_) => "Int".into(), Self::String(_) => "String".into(),
            Self::Bool(_) => "Bool".into(), Self::None => "None".into(),
            Self::Function(_) => "Function".into(),
            Self::List(items) => format!("List[{}]", items.first().map(Self::type_name).unwrap_or_else(|| "Unknown".into())),
        }
    }
}

impl std::fmt::Display for EvaluationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => write!(formatter, "invalid expression, target, or arguments"),
            Self::Thrown(value) => write!(formatter, "uncaught exception: {}", value),
        }
    }
}
impl std::error::Error for EvaluationFailure {}

#[derive(Clone, Copy, Default)]
struct EvaluationContext<'a> {
    signal: Option<&'a ReceivedSignal>,
    loop_name: Option<&'a str>,
    types: Option<&'a HashMap<String, String>>,
}

impl SignalRouter {
    /// Registers a user-defined function for expression evaluation.
    pub fn register_function(&self, function: FuncDecl) {
        self.functions.lock().expect("function registry lock poisoned")
            .insert(function.name.clone(), function);
    }

    /// Evaluates a Lia function call. Standard-library operations are
    /// available only with the qualified `std.` name, so source files must
    /// import `std` before using them.
    pub fn evaluate_function(&self, name: &str, args: Vec<RuntimeValue>) -> Option<RuntimeValue> {
        self.evaluate_function_result(name, args).ok()
    }

    pub fn evaluate_function_result(&self, name: &str, args: Vec<RuntimeValue>) -> Result<RuntimeValue, EvaluationFailure> {
        if crate::stdlib::is_total_standard_function(name) {
            // A direct runtime call cannot bypass package resolution either:
            // only a registered, imported declaration unlocks `std.*`.
            if !self.functions.lock().expect("function registry lock poisoned").contains_key(name) {
                return Err(EvaluationFailure::Invalid);
            }
            return Self::evaluate_std_function(name, &args).ok_or(EvaluationFailure::Invalid);
        }
        let function = self.functions.lock().expect("function registry lock poisoned").get(name).cloned().ok_or(EvaluationFailure::Invalid)?;
        if function.params.len() != args.len() { return Err(EvaluationFailure::Invalid); }
        let mut scope = HashMap::new();
        for (param, value) in function.params.iter().zip(args) {
            scope.insert(param.name.clone(), value);
        }
        Self::evaluate_block(&function.body, &mut scope, self)
    }

    /// Evaluates a closed expression supplied by a routine (for example, a
    /// literal list passed to a user-defined function from `main`).
    pub fn evaluate_expression(&self, expression: &Expr) -> Option<RuntimeValue> {
        Self::evaluate_expr(expression, &HashMap::new(), self).ok()
    }

    /// Evaluate an expression using the values local to the caller.
    pub fn evaluate_expression_in_scope(&self, expression: &Expr, scope: &HashMap<String, RuntimeValue>) -> Option<RuntimeValue> {
        Self::evaluate_expr(expression, scope, self).ok()
    }

    /// Evaluates with explicit failures instead of substituting a missing value.
    pub fn evaluate_expression_result(&self, expression: &Expr, scope: &HashMap<String, RuntimeValue>) -> Result<RuntimeValue, EvaluationFailure> {
        Self::evaluate_expr(expression, scope, self)
    }

    pub(crate) fn evaluate_expression_with_context(&self, expression: &Expr, scope: &HashMap<String, RuntimeValue>, signal: Option<&ReceivedSignal>, loop_name: Option<&str>, types: &HashMap<String, String>) -> Result<RuntimeValue, EvaluationFailure> {
        Self::evaluate_expr_context(expression, scope, self, EvaluationContext { signal, loop_name, types: Some(types) })
    }

    pub(crate) fn report_error(&self, error: String) {
        eprintln!("Runtime error: {}", error);
        self.errors.lock().expect("runtime errors lock poisoned").push(error);
    }

    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut *self.errors.lock().expect("runtime errors lock poisoned"))
    }

    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.lock().expect("runtime errors lock poisoned").is_empty()
    }

    fn evaluate_block(
        body: &[Statement],
        scope: &mut HashMap<String, RuntimeValue>,
        router: &SignalRouter,
    ) -> Result<RuntimeValue, EvaluationFailure> {
        let mut result = RuntimeValue::None;
        for statement in body {
            match statement {
                Statement::VarDecl { name, value, .. } => {
                    let value = Self::evaluate_expr(value, scope, router)?;
                    scope.insert(name.clone(), value);
                }
                Statement::MemorySet { .. } => return Err(EvaluationFailure::Invalid),
                Statement::ExprStmt(expr) => result = Self::evaluate_expr(expr, scope, router)?,
                Statement::Match { target, cases } => {
                    let value = Self::evaluate_expr(target, scope, router)?;
                    let case = cases.iter().find(|case| Self::pattern_matches(&case.pattern, &value)).ok_or(EvaluationFailure::Invalid)?;
                    let mut branch_scope = scope.clone();
                    Self::bind_pattern(&case.pattern, &value, &mut branch_scope);
                    result = Self::evaluate_block(&case.body, &mut branch_scope, router)?;
                }
                Statement::ForLoop { var_name, collection, body } => {
                    let RuntimeValue::List(items) = Self::evaluate_expr(collection, scope, router)? else { return Err(EvaluationFailure::Invalid); };
                    for item in items {
                        let mut iteration_scope = scope.clone();
                        iteration_scope.insert(var_name.clone(), item);
                        result = Self::evaluate_block(body, &mut iteration_scope, router)?;
                    }
                }
                Statement::TryCatch { try_body, error_name, catch_body } => {
                    let mut try_scope = scope.clone();
                    result = match Self::evaluate_block(try_body, &mut try_scope, router) {
                        Ok(value) => value,
                        Err(EvaluationFailure::Thrown(value)) => {
                            let mut catch_scope = scope.clone();
                            catch_scope.insert(error_name.clone(), value);
                            Self::evaluate_block(catch_body, &mut catch_scope, router)?
                        }
                        Err(error) => return Err(error),
                    };
                }
                Statement::Throw(expr) => return Err(EvaluationFailure::Thrown(Self::evaluate_expr(expr, scope, router)?)),
                Statement::Signal { loop_name, payload } => {
                    let payload = Self::evaluate_expr(payload, scope, router)?;
                    router.emit(loop_name, payload.to_string());
                }
                Statement::Spawn(_) | Statement::Terminate => return Err(EvaluationFailure::Invalid),
            }
        }
        Ok(result)
    }

    fn evaluate_expr(expr: &Expr, scope: &HashMap<String, RuntimeValue>, router: &SignalRouter) -> Result<RuntimeValue, EvaluationFailure> {
        Self::evaluate_expr_context(expr, scope, router, EvaluationContext::default())
    }

    fn evaluate_expr_context(expr: &Expr, scope: &HashMap<String, RuntimeValue>, router: &SignalRouter, context: EvaluationContext<'_>) -> Result<RuntimeValue, EvaluationFailure> {
        match expr {
            Expr::IntLiteral(value) => Ok(RuntimeValue::Int(*value)),
            Expr::StringLiteral(value) => Ok(RuntimeValue::String(value.clone())),
            Expr::ListLiteral(items) => Ok(RuntimeValue::List(items.iter().map(|item| Self::evaluate_expr_context(item, scope, router, context)).collect::<Result<Vec<_>, _>>()?)),
            Expr::Variable(name) if Self::qualified_is_shadowed(name, scope) => Err(EvaluationFailure::Invalid),
            Expr::Variable(name) => match name.as_str() {
                "True" => Ok(RuntimeValue::Bool(true)),
                "False" => Ok(RuntimeValue::Bool(false)),
                _ => scope.get(name).cloned().or_else(|| {
                    router.functions.lock().expect("function registry lock poisoned")
                        .contains_key(name).then(|| RuntimeValue::Function(name.clone()))
                }).ok_or(EvaluationFailure::Invalid),
            },
            Expr::TypeOf(name) => Ok(RuntimeValue::String(context.types.and_then(|types| types.get(name)).cloned().or_else(|| scope.get(name).map(RuntimeValue::type_name)).ok_or(EvaluationFailure::Invalid)?)),
            Expr::FunctionCall { name, args } => {
                if Self::qualified_is_shadowed(name, scope) { return Err(EvaluationFailure::Invalid); }
                let target = match scope.get(name) {
                    Some(RuntimeValue::Function(target)) => target.as_str(),
                    Some(_) => return Err(EvaluationFailure::Invalid),
                    None => name.as_str(),
                };
                router.evaluate_function_result(target, args.iter().map(|arg| Self::evaluate_expr_context(arg, scope, router, context)).collect::<Result<Vec<_>, _>>()?)
            }
            Expr::IndirectCall { target, args } => {
                let RuntimeValue::Function(name) = Self::evaluate_expr_context(target, scope, router, context)? else { return Err(EvaluationFailure::Invalid); };
                router.evaluate_function_result(&name, args.iter().map(|arg| Self::evaluate_expr_context(arg, scope, router, context)).collect::<Result<Vec<_>, _>>()?)
            }
            // Type arguments are checked statically. Named functions carry
            // no captured state, so instantiation preserves their identity.
            Expr::Specialize { target, .. } => Self::evaluate_expr_context(target, scope, router, context),
            Expr::BinaryOp { op, left, right } => {
                let left = Self::evaluate_expr_context(left, scope, router, context)?;
                let right = Self::evaluate_expr_context(right, scope, router, context)?;
                match (op.as_str(), left, right) {
                    ("+", RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.checked_add(b).map(RuntimeValue::Int),
                    ("-", RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.checked_sub(b).map(RuntimeValue::Int),
                    ("*", RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.checked_mul(b).map(RuntimeValue::Int),
                    ("/", RuntimeValue::Int(a), RuntimeValue::Int(b)) => a.checked_div(b).map(RuntimeValue::Int),
                    ("==", a, b) => Some(RuntimeValue::Bool(a == b)),
                    ("!=", a, b) => Some(RuntimeValue::Bool(a != b)),
                    ("<", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Bool(a < b)),
                    ("<=", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Bool(a <= b)),
                    (">", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Bool(a > b)),
                    (">=", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Bool(a >= b)),
                    _ => None,
                }.ok_or(EvaluationFailure::Invalid)
            }
            Expr::RefCreate(inner) => Self::evaluate_expr_context(inner, scope, router, context),
            Expr::MemberAccess { target, field } if matches!(target.as_ref(), Expr::ReceiveSignal) => match field.as_str() {
                "value" => Ok(context.signal.and_then(|signal| signal.value.clone()).map(RuntimeValue::String).unwrap_or(RuntimeValue::None)),
                "pending" => Ok(RuntimeValue::Int(context.signal.map_or(0, |signal| signal.pending) as i64)),
                _ => Err(EvaluationFailure::Invalid),
            },
            Expr::MemberAccess { target, field } => {
                fn qualified(expr: &Expr) -> Option<String> {
                    match expr {
                        Expr::Variable(name) => Some(name.clone()),
                        Expr::MemberAccess { target, field } => Some(format!("{}.{}", qualified(target)?, field)),
                        _ => None,
                    }
                }
                let name = format!("{}.{}", qualified(target).ok_or(EvaluationFailure::Invalid)?, field);
                if Self::qualified_is_shadowed(&name, scope) { return Err(EvaluationFailure::Invalid); }
                if router.functions.lock().expect("function registry lock poisoned").contains_key(&name) {
                    Ok(RuntimeValue::Function(name))
                } else { Err(EvaluationFailure::Invalid) }
            }
            Expr::ReceiveSignal => Ok(context.signal.and_then(|signal| signal.value.clone()).map(RuntimeValue::String).unwrap_or(RuntimeValue::None)),
            Expr::MemoryGet(key) => {
                let loop_name = context.loop_name.ok_or(EvaluationFailure::Invalid)?;
                Ok(router.memory_get(loop_name, key).map(RuntimeValue::String).unwrap_or(RuntimeValue::None))
            }
        }
    }

    fn qualified_is_shadowed(name: &str, scope: &HashMap<String, RuntimeValue>) -> bool {
        name.split_once('.').is_some_and(|(root, _)| scope.contains_key(root))
    }

    pub(crate) fn pattern_matches(pattern: &Pattern, value: &RuntimeValue) -> bool {
        match (pattern, value) {
            (Pattern::Wildcard | Pattern::Identifier(_), _) => true,
            (Pattern::Constructor(name, patterns), RuntimeValue::List(items)) if name == "Nil" => patterns.is_empty() && items.is_empty(),
            (Pattern::Constructor(name, patterns), RuntimeValue::List(items)) if name == "Cons" && patterns.len() == 2 => {
                items.split_first().is_some_and(|(head, tail)| Self::pattern_matches(&patterns[0], head) && Self::pattern_matches(&patterns[1], &RuntimeValue::List(tail.to_vec())))
            }
            (Pattern::Constructor(name, patterns), RuntimeValue::Bool(value)) if name == "True" => patterns.is_empty() && *value,
            (Pattern::Constructor(name, patterns), RuntimeValue::Bool(value)) if name == "False" => patterns.is_empty() && !*value,
            _ => false,
        }
    }

    pub(crate) fn bind_pattern(pattern: &Pattern, value: &RuntimeValue, scope: &mut HashMap<String, RuntimeValue>) {
        match pattern {
            Pattern::Identifier(name) => { scope.insert(name.clone(), value.clone()); }
            Pattern::Constructor(name, patterns) if name == "Cons" && patterns.len() == 2 => {
                if let RuntimeValue::List(items) = value {
                    if let Some((head, tail)) = items.split_first() {
                        Self::bind_pattern(&patterns[0], head, scope);
                        Self::bind_pattern(&patterns[1], &RuntimeValue::List(tail.to_vec()), scope);
                    }
                }
            }
            _ => {}
        }
    }

    fn evaluate_std_function(name: &str, args: &[RuntimeValue]) -> Option<RuntimeValue> {
        match (name, args) {
            ("std.equals", [left, right]) => Some(RuntimeValue::Bool(left == right)),
            ("std.less_or_equal", [RuntimeValue::Int(left), RuntimeValue::Int(right)]) => Some(RuntimeValue::Bool(left <= right)),
            ("std.max", [RuntimeValue::Int(left), RuntimeValue::Int(right)]) => Some(RuntimeValue::Int((*left).max(*right))),
            ("std.empty", []) => Some(RuntimeValue::List(Vec::new())),
            ("std.prepend", [value, RuntimeValue::List(items)]) => {
                let mut values = Vec::with_capacity(items.len() + 1);
                values.push(value.clone());
                values.extend(items.clone());
                Some(RuntimeValue::List(values))
            }
            ("std.log", values) => {
                println!("{}", values.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "));
                Some(RuntimeValue::None)
            },
            _ => None,
        }
    }
    /// Registers a routine body for later asynchronous spawning.
    pub fn register_routine(&self, name: &str, statements: Vec<Statement>) -> bool {
        self.register_routine_with_params(name, Vec::new(), statements)
    }

    pub fn register_routine_with_params(&self, name: &str, params: Vec<Param>, statements: Vec<Statement>) -> bool {
        let mut routines = self.routines.lock().expect("routine registry lock poisoned");
        if routines.contains_key(name) { return false; }
        routines.insert(name.to_string(), (params, statements));
        true
    }

    /// Creates a fresh routine task. The cloned AST is intentional: each task
    /// receives its own arena when a worker begins execution.
    pub fn spawn_routine(&self, name: &str, payload: String) -> Option<Message> {
        self.spawn_routine_with_args(name, payload, Vec::new())
    }

    pub fn spawn_routine_with_args(&self, name: &str, payload: String, args: Vec<RuntimeValue>) -> Option<Message> {
        self.routines.lock().expect("routine registry lock poisoned").get(name).and_then(|(params, statements)| (params.len() == args.len()).then(|| Message {
            routine_name: name.to_string(),
            payload,
            statements: statements.clone(),
            is_loop: false,
            bindings: params.iter().zip(args).map(|(param, value)| (param.name.clone(), value)).collect(),
        }))
    }
    /// Registers one addressable loop name. A duplicate name is rejected
    /// instead of sharing a mailbox between independent loop instances.
    pub fn register_loop(&self, name: &str, statements: Vec<Statement>) -> bool {
        let mut mailboxes = self.mailboxes.lock().expect("signal mailbox lock poisoned");
        if mailboxes.contains_key(name) {
            return false;
        }
        let (sender, receiver) = crossbeam_channel::unbounded();
        mailboxes.insert(name.to_string(), LoopMailbox { sender, receiver, active: false, statements });
        true
    }

    /// Claims the sole running instance for this loop name.
    pub fn start_loop(&self, name: &str) -> bool {
        let mut mailboxes = self.mailboxes.lock().expect("signal mailbox lock poisoned");
        match mailboxes.get_mut(name) {
            Some(mailbox) if !mailbox.active => {
                mailbox.active = true;
                true
            }
            _ => false,
        }
    }

    /// Marks the named loop dormant after `terminate`, allowing a later spawn
    /// to start a new instance using the same mailbox.
    pub fn stop_loop(&self, name: &str) {
        if let Some(mailbox) = self.mailboxes.lock().expect("signal mailbox lock poisoned").get_mut(name) {
            mailbox.active = false;
        }
        self.loop_memory.lock().expect("loop memory lock poisoned").remove(name);
    }

    pub fn memory_get(&self, loop_name: &str, key: &str) -> Option<String> {
        self.loop_memory.lock().expect("loop memory lock poisoned")
            .get(loop_name).and_then(|memory| memory.get(key)).cloned()
    }

    pub fn memory_set(&self, loop_name: &str, key: String, value: String) {
        self.loop_memory.lock().expect("loop memory lock poisoned")
            .entry(loop_name.to_string()).or_default().insert(key, value);
    }

    pub fn has_loop(&self, name: &str) -> bool {
        self.mailboxes.lock().expect("signal mailbox lock poisoned").contains_key(name)
    }

    /// A loop declaration is dormant until spawned. The first `spawn` claims
    /// and schedules it; later spawns only enqueue their optional value as a
    /// signal, so the running loop continues from its current iteration flow.
    pub fn spawn_or_signal(&self, name: &str, payload: Option<String>) -> LoopSpawn {
        let mut mailboxes = self.mailboxes.lock().expect("signal mailbox lock poisoned");
        let Some(mailbox) = mailboxes.get_mut(name) else {
            return LoopSpawn::Unknown;
        };

        if let Some(value) = payload {
            // Unbounded channel send does not wait for the loop.
            if mailbox.sender.send(value).is_err() {
                return LoopSpawn::Unknown;
            }
        }

        if mailbox.active {
            LoopSpawn::Signalled
        } else {
            mailbox.active = true;
            LoopSpawn::Started(Message {
                routine_name: name.to_string(),
                payload: format!("Loop {} started", name),
                statements: mailbox.statements.clone(),
                is_loop: true,
                bindings: HashMap::new(),
            })
        }
    }

    /// Returns false if no loop with this name is registered. Sending itself
    /// is asynchronous with respect to loop execution.
    pub fn emit(&self, loop_name: &str, payload: String) -> bool {
        // Never wait behind registry maintenance: an emitter either obtains
        // the mailbox immediately or drops the signal and continues.
        let sender = match self.mailboxes.try_lock() {
            Ok(mailboxes) => mailboxes.get(loop_name).map(|mailbox| mailbox.sender.clone()),
            Err(_) => return false,
        };
        sender.map(|sender| sender.send(payload).is_ok()).unwrap_or(false)
    }

    /// Called immediately before a loop iteration starts. Exactly one value is
    /// delivered so the next iteration can receive the next queued signal.
    pub fn receive_next(&self, loop_name: &str) -> ReceivedSignal {
        let receiver = self.mailboxes.lock().expect("signal mailbox lock poisoned")
            .get(loop_name).map(|mailbox| mailbox.receiver.clone());
        match receiver {
            Some(receiver) => ReceivedSignal {
                value: receiver.try_recv().ok(),
                pending: receiver.len(),
            },
            None => ReceivedSignal { value: None, pending: 0 },
        }
    }
}

impl ActorHandle {
    pub fn new(sender: mpsc::Sender<Message>) -> Self {
        Self { sender }
    }

    pub async fn send_spawn(&self, msg: Message) -> Result<(), String> {
        self.sender
            .send(msg)
            .await
            .map_err(|e| format!("Failed to dispatch spawn for routine: {}", e))
    }
}
