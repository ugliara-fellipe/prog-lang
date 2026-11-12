use std::collections::HashMap;
use std::sync::Mutex;
use crossbeam_channel::{Receiver, Sender};
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use crate::ast::Statement;

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

impl SignalRouter {
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
