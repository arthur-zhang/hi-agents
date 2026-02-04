mod codex;
mod claude;
mod ws;

use agent_client_protocol::{NewSessionResponse, RequestPermissionRequest, SessionUpdate};
use uuid::uuid;

fn main() {
    println!("Hello, world!");
    let a = agent_client_protocol::NewSessionRequest::new("/tmp");
    let id = uuid::Uuid::new_v4().to_string();
    let b : NewSessionResponse = NewSessionResponse::new(id);
    println!("{:#?}", a);
    println!("{:#?}", b);

}
