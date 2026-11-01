use tokio::sync::mpsc;

pub enum ActorMessage {
    SetValue(String),
    GetVal(tokio::sync::oneshot::Sender<String>),
}

pub struct ActorHandle {
    sender: mpsc::Sender<ActorMessage>,
}

impl ActorHandle {
    pub async fn set_value(&self, val: String) {
        let _ = self.sender.send(ActorMessage::SetValue(val)).await;
    }
}