use std::collections::HashMap;
use std::sync::Mutex;
use crossbeam_channel::{Receiver, Sender};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use crate::ast::{Expr, FuncDecl, Pattern, Statement};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Name used for diagnostics and loop-mailbox lookup.
    pub routine_name: String,
    pub payload: String,
    #[serde(default)]
    pub statements: Vec<Statement>,
    #[serde(default)]
    pub is_loop: bool,
}

pub struct ActorHandle {
    sender: mpsc::Sender<Message>,
}

/// Mailboxes for persistent loops. `Sender::send` on an unbounded channel
/// never waits for the receiving loop, so emitting a signal cannot block its
/// caller.
#[derive(Default)]
pub struct SignalRouter {
    /// One FIFO mailbox plus lifecycle state for each declared loop.
    mailboxes: Mutex<HashMap<String, LoopMailbox>>,
    /// Routine bodies are registered once at startup so `spawn Worker()` can
    /// materialize an independent task without sharing local arena state.
    routines: Mutex<HashMap<String, Vec<Statement>>>,
    /// User functions are interpreted from their AST. Runtime intrinsics are
    /// deliberately limited to the explicitly imported `std.*` namespace.
    functions: Mutex<HashMap<String, FuncDecl>>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    Int(i64),
    String(String),
    Bool(bool),
    List(Vec<RuntimeValue>),
    None,
}

impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeValue::Int(value) => write!(formatter, "{}", value),
            RuntimeValue::String(value) => write!(formatter, "{}", value),
            RuntimeValue::Bool(value) => write!(formatter, "{}", value),
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

impl SignalRouter {
    /// Registers a user-defined function for expression evaluation.
    pub fn register_function(&self, function: FuncDecl) {
        self.functions.lock().expect("function registry lock poisoned")
            .insert(function.name.clone(), function);
    }

    /// Evaluates a ProgLang function call. Standard-library operations are
    /// available only with the qualified `std.` name, so source files must
    /// import `std` before using them.
    pub fn evaluate_function(&self, name: &str, args: Vec<RuntimeValue>) -> Option<RuntimeValue> {
        if crate::stdlib::is_total_standard_function(name) {
            // A direct runtime call cannot bypass package resolution either:
            // only a registered, imported declaration unlocks `std.*`.
            if !self.functions.lock().expect("function registry lock poisoned").contains_key(name) {
                return None;
            }
            return Self::evaluate_std_function(name, &args);
        }
        let function = self.functions.lock().expect("function registry lock poisoned").get(name).cloned()?;
        if function.params.len() != args.len() { return None; }
        let mut scope = HashMap::new();
        for (param, value) in function.params.iter().zip(args) {
            scope.insert(param.name.clone(), value);
        }
        Self::evaluate_block(&function.body, &mut scope, self)
    }

    /// Evaluates a closed expression supplied by a routine (for example, a
    /// literal list passed to a user-defined function from `main`).
    pub fn evaluate_expression(&self, expression: &Expr) -> Option<RuntimeValue> {
        Self::evaluate_expr(expression, &HashMap::new(), self)
    }

    fn evaluate_block(
        body: &[Statement],
        scope: &mut HashMap<String, RuntimeValue>,
        router: &SignalRouter,
    ) -> Option<RuntimeValue> {
        let mut result = RuntimeValue::None;
        for statement in body {
            match statement {
                Statement::VarDecl { name, value, .. } => {
                    let value = Self::evaluate_expr(value, scope, router)?;
                    scope.insert(name.clone(), value);
                }
                Statement::ExprStmt(expr) => result = Self::evaluate_expr(expr, scope, router)?,
                Statement::Match { target, cases } => {
                    let value = Self::evaluate_expr(target, scope, router)?;
                    let case = cases.iter().find(|case| Self::pattern_matches(&case.pattern, &value))?;
                    let mut branch_scope = scope.clone();
                    Self::bind_pattern(&case.pattern, &value, &mut branch_scope);
                    result = Self::evaluate_block(&case.body, &mut branch_scope, router)?;
                }
                Statement::ForLoop { var_name, collection, body } => {
                    let RuntimeValue::List(items) = Self::evaluate_expr(collection, scope, router)? else { return None; };
                    for item in items {
                        scope.insert(var_name.clone(), item);
                        result = Self::evaluate_block(body, scope, router)?;
                    }
                }
                Statement::TryCatch { try_body, catch_body, .. } => {
                    // Function calls are currently total values in this small
                    // interpreter; explicit throws have no value to return.
                    result = Self::evaluate_block(try_body, scope, router)
                        .or_else(|| Self::evaluate_block(catch_body, scope, router))?;
                }
                Statement::Throw(_) => return None,
                Statement::Spawn(_) | Statement::Signal { .. } | Statement::Terminate => {}
            }
        }
        Some(result)
    }

    fn evaluate_expr(expr: &Expr, scope: &HashMap<String, RuntimeValue>, router: &SignalRouter) -> Option<RuntimeValue> {
        match expr {
            Expr::IntLiteral(value) => Some(RuntimeValue::Int(*value)),
            Expr::StringLiteral(value) => Some(RuntimeValue::String(value.clone())),
            Expr::ListLiteral(items) => Some(RuntimeValue::List(items.iter().map(|item| Self::evaluate_expr(item, scope, router)).collect::<Option<Vec<_>>>()?)),
            Expr::Variable(name) => match name.as_str() {
                "True" => Some(RuntimeValue::Bool(true)),
                "False" => Some(RuntimeValue::Bool(false)),
                _ => scope.get(name).cloned(),
            },
            Expr::FunctionCall { name, args } => router.evaluate_function(name, args.iter().map(|arg| Self::evaluate_expr(arg, scope, router)).collect::<Option<Vec<_>>>()?),
            Expr::BinaryOp { op, left, right } => {
                let left = Self::evaluate_expr(left, scope, router)?;
                let right = Self::evaluate_expr(right, scope, router)?;
                match (op.as_str(), left, right) {
                    ("+", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Int(a + b)),
                    ("-", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Int(a - b)),
                    ("*", RuntimeValue::Int(a), RuntimeValue::Int(b)) => Some(RuntimeValue::Int(a * b)),
                    ("/", RuntimeValue::Int(a), RuntimeValue::Int(b)) if b != 0 => Some(RuntimeValue::Int(a / b)),
                    _ => None,
                }
            }
            Expr::RefCreate(inner) => Self::evaluate_expr(inner, scope, router),
            Expr::MemberAccess { .. } | Expr::ReceiveSignal => None,
        }
    }

    fn pattern_matches(pattern: &Pattern, value: &RuntimeValue) -> bool {
        match (pattern, value) {
            (Pattern::Wildcard | Pattern::Identifier(_), _) => true,
            (Pattern::Constructor(name, _), RuntimeValue::List(items)) if name == "Nil" => items.is_empty(),
            (Pattern::Constructor(name, patterns), RuntimeValue::List(items)) if name == "Cons" && patterns.len() == 2 => !items.is_empty(),
            (Pattern::Constructor(name, _), RuntimeValue::Bool(value)) if name == "True" => *value,
            (Pattern::Constructor(name, _), RuntimeValue::Bool(value)) if name == "False" => !*value,
            _ => false,
        }
    }

    fn bind_pattern(pattern: &Pattern, value: &RuntimeValue, scope: &mut HashMap<String, RuntimeValue>) {
        match pattern {
            Pattern::Identifier(name) => { scope.insert(name.clone(), value.clone()); }
            Pattern::Constructor(name, patterns) if name == "Cons" => {
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
            // Terminal output is dispatched by the routine executor. This
            // marker still makes the runtime primitive explicit and finite.
            ("std.log", _) => Some(RuntimeValue::None),
            _ => None,
        }
    }
    /// Registers a routine body for later asynchronous spawning.
    pub fn register_routine(&self, name: &str, statements: Vec<Statement>) -> bool {
        let mut routines = self.routines.lock().expect("routine registry lock poisoned");
        if routines.contains_key(name) { return false; }
        routines.insert(name.to_string(), statements);
        true
    }

    /// Creates a fresh routine task. The cloned AST is intentional: each task
    /// receives its own arena when a worker begins execution.
    pub fn spawn_routine(&self, name: &str, payload: String) -> Option<Message> {
        self.routines.lock().expect("routine registry lock poisoned").get(name).map(|statements| Message {
            routine_name: name.to_string(),
            payload,
            statements: statements.clone(),
            is_loop: false,
        })
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
