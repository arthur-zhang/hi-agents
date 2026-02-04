# WebSocket Session 服务实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 创建 WebSocket 服务，处理 NewSessionRequest 并返回 NewSessionResponse

**Architecture:** 单模块实现，使用 axum WebSocket + DashMap 存储 session。客户端发送 JSON 消息，服务端解析后处理并返回响应。

**Tech Stack:** Rust, axum (ws), tokio, serde_json, dashmap, uuid, agent-client-protocol

---

### Task 1: 创建消息类型定义

**Files:**
- Create: `src/ws.rs`

**Step 1: 创建 ws.rs 并定义消息类型**

```rust
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
```

**Step 2: 在 main.rs 中添加模块声明**

在 `src/main.rs` 顶部添加：
```rust
mod ws;
```

**Step 3: 验证编译通过**

Run: `cargo check`
Expected: 编译成功，无错误

**Step 4: Commit**

```bash
git add src/ws.rs src/main.rs
git commit -m "feat(ws): add client/server message types"
```

---

### Task 2: 实现 SessionState 和 SessionManager

**Files:**
- Modify: `src/ws.rs`

**Step 1: 添加 SessionState 和类型别名**

在 `src/ws.rs` 中添加：

```rust
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub workspace_dir: PathBuf,
    pub created_at: DateTime<Utc>,
}

pub type SessionManager = Arc<DashMap<String, SessionState>>;
```

**Step 2: 验证编译通过**

Run: `cargo check`
Expected: 编译成功

**Step 3: Commit**

```bash
git add src/ws.rs
git commit -m "feat(ws): add SessionState and SessionManager types"
```

---

### Task 3: 实现 WebSocket Handler

**Files:**
- Modify: `src/ws.rs`

**Step 1: 添加 WebSocket handler 函数**

在 `src/ws.rs` 中添加：

```rust
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use uuid::Uuid;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(sessions): State<SessionManager>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, sessions))
}

async fn handle_socket(mut socket: WebSocket, sessions: SessionManager) {
    let mut current_session_id: Option<String> = None;

    while let Some(msg) = socket.recv().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text,
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => continue,
        };

        let response = match serde_json::from_str::<ClientMessage>(&msg) {
            Ok(ClientMessage::NewSession(req)) => {
                let session_id = Uuid::new_v4().to_string();
                let state = SessionState {
                    session_id: session_id.clone(),
                    workspace_dir: req.cwd.clone(),
                    created_at: Utc::now(),
                };
                sessions.insert(session_id.clone(), state);
                current_session_id = Some(session_id.clone());

                ServerMessage::NewSession(NewSessionResponse::new(session_id))
            }
            Err(e) => ServerMessage::Error {
                message: format!("Invalid message: {}", e),
            },
        };

        let response_text = serde_json::to_string(&response).unwrap();
        if socket.send(Message::Text(response_text.into())).await.is_err() {
            break;
        }
    }

    // 清理 session
    if let Some(session_id) = current_session_id {
        sessions.remove(&session_id);
        tracing::info!("Session {} cleaned up", session_id);
    }
}
```

**Step 2: 验证编译通过**

Run: `cargo check`
Expected: 编译成功

**Step 3: Commit**

```bash
git add src/ws.rs
git commit -m "feat(ws): implement WebSocket handler with session management"
```

---

### Task 4: 更新 main.rs 启动服务器

**Files:**
- Modify: `src/main.rs`

**Step 1: 重写 main.rs**

```rust
mod claude;
mod codex;
mod ws;

use axum::{routing::get, Router};
use dashmap::DashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let sessions = Arc::new(DashMap::new());

    let app = Router::new()
        .route("/ws", get(ws::ws_handler))
        .with_state(sessions);

    let addr = "0.0.0.0:3000";
    tracing::info!("WebSocket server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**Step 2: 验证编译通过**

Run: `cargo check`
Expected: 编译成功

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: update main.rs to start WebSocket server"
```

---

### Task 5: 手动测试验证

**Step 1: 启动服务器**

Run: `cargo run`
Expected: 输出 "WebSocket server listening on 0.0.0.0:3000"

**Step 2: 使用 websocat 测试**

在另一个终端运行：
```bash
websocat ws://127.0.0.1:3000/ws
```

发送消息：
```json
{"type":"new_session","cwd":"/tmp","mcp_servers":[]}
```

Expected: 收到类似响应：
```json
{"type":"new_session","session_id":"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"}
```

**Step 3: 验证 session 清理**

断开 websocat 连接，检查服务器日志应显示 "Session xxx cleaned up"

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: complete WebSocket session service implementation"
```

---

## 完成检查清单

- [ ] Task 1: 消息类型定义
- [ ] Task 2: SessionState 和 SessionManager
- [ ] Task 3: WebSocket Handler
- [ ] Task 4: main.rs 更新
- [ ] Task 5: 手动测试验证
