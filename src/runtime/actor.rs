use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use crate::ast::Statement;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub routine_name: String,
    pub payload: String,
    #[serde(default)]
    pub statements: Vec<Statement>,
}

pub struct ActorHandle {
    sender: mpsc::Sender<Message>,
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
