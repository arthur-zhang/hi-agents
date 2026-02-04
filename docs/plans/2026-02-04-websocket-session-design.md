# WebSocket Session 服务设计

## 概述

创建一个 WebSocket 服务，处理 `NewSessionRequest` 并返回 `NewSessionResponse`，支持持久会话连接。

## 需求

- 持久会话的 WebSocket 服务
- 目前只实现 `NewSessionRequest/Response`，后续扩展其他消息类型
- 简单会话管理：连接断开时清理
- 独立运行：`main.rs` 启动 WebSocket 服务器

## 架构

### 方案选择

采用单模块实现（方案 A）：
- 在 `src/` 下新建 `ws.rs` 模块
- 使用 `DashMap` 存储 session
- 直接在 `main.rs` 启动 axum 服务器

### 组件

1. **WebSocket Handler** - 处理 WebSocket 连接升级和消息路由
2. **SessionManager** - 使用 `DashMap<String, SessionState>` 存储活跃会话
3. **消息处理** - 解析 JSON 消息，分发到对应的处理函数

### 数据流

```
客户端连接
  → axum WebSocket 升级
  → 等待 NewSessionRequest (JSON)
  → 生成 session_id (UUID)
  → 存储到 SessionManager
  → 返回 NewSessionResponse (JSON)
  → 保持连接，等待后续消息
  → 连接断开时从 SessionManager 清理
```

### SessionState 结构

```rust
struct SessionState {
    session_id: String,        // 会话唯一标识
    workspace_dir: PathBuf,    // 工作目录
    created_at: DateTime<Utc>, // 创建时间
}
```

## 消息协议

使用 JSON over WebSocket。

```rust
// 客户端 → 服务端
enum ClientMessage {
    NewSession(NewSessionRequest),
    // 后续扩展其他消息类型
}

// 服务端 → 客户端
enum ServerMessage {
    NewSession(NewSessionResponse),
    Error { message: String },
}
```

## 错误处理

1. **连接级错误**（无法恢复）：
   - JSON 解析失败 → 发送 Error 消息后关闭连接
   - 未知消息类型 → 发送 Error 消息后关闭连接

2. **业务级错误**（可恢复）：
   - 工作目录不存在 → 发送 Error 消息，保持连接
   - Session 已存在 → 发送 Error 消息，保持连接

3. **连接断开**：
   - 正常关闭或异常断开都触发清理
   - 从 SessionManager 移除对应 session

## 实现结构

### main.rs

```rust
#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let sessions = Arc::new(DashMap::new());

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(sessions);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### ws.rs 核心函数

1. `ws_handler()` - 处理 WebSocket 升级请求
2. `handle_socket()` - 主消息循环
3. `process_message()` - 解析并路由消息
4. `handle_new_session()` - 处理 NewSessionRequest

## 并发安全

1. **SessionManager 使用 DashMap**：线程安全，支持并发读写
2. **每个连接独立处理**：axum 为每个连接创建独立 task
3. **清理时机**：在 `handle_socket()` 结束时确保清理

## 测试策略

1. **单元测试**：消息序列化/反序列化，session 创建和清理
2. **集成测试**：使用 `tokio-tungstenite` 测试完整流程
3. **手动测试**：使用 `websocat` 验证基本功能
