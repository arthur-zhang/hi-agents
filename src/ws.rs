use agent_client_protocol::{NewSessionRequest, NewSessionResponse};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    NewSession(NewSessionRequest),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    NewSession(NewSessionResponse),
    Error { message: String },
}
