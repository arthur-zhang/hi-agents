import WebSocket from "ws";

const uri = "ws://127.0.0.1:3000/ws";

let requestId = 1;

function createJsonRpcRequest(method: string, params: any) {
  return {
    jsonrpc: "2.0",
    id: requestId++,
    method,
    params,
  };
}

async function testWebSocket() {
  return new Promise<void>((resolve, reject) => {
    const ws = new WebSocket(uri);

    ws.on("open", () => {
      // 发送 session/new 请求
      const request = createJsonRpcRequest("session/new", {
        cwd: "/tmp",
        mcpServers: [],
      });
      ws.send(JSON.stringify(request));
      console.log(`Sent: ${JSON.stringify(request, null, 2)}`);
    });

    ws.on("message", (data) => {
      const response = data.toString();
      console.log(`Received: ${response}`);

      // 解析 JSON-RPC 响应
      const responseData = JSON.parse(response);

      if (responseData.result) {
        // 成功响应
        console.log(`✅ Success! Result:`, responseData.result);

        // 如果是 session/new 的响应，发送 session/prompt
        if (responseData.result.sessionId) {
          const promptRequest = createJsonRpcRequest("session/prompt", {
            sessionId: responseData.result.sessionId,
            prompt: [{ type: "text", text: "Hello, Claude!" }],
          });
          ws.send(JSON.stringify(promptRequest));
          console.log(`Sent prompt: ${JSON.stringify(promptRequest, null, 2)}`);
        }
      } else if (responseData.error) {
        // 错误响应
        console.log(`❌ Error: ${responseData.error.message}`);
      } else if (responseData.method) {
        // 通知
        console.log(`📢 Notification [${responseData.method}]:`, responseData.params);
      }
    });

    ws.on("error", (err) => {
      console.log(`❌ Error: ${err.message}`);
      reject(err);
    });

    ws.on("close", () => {
      console.log("Connection closed");
      resolve();
    });

    // 30秒后自动关闭
    setTimeout(() => {
      console.log("Timeout, closing connection...");
      ws.close();
    }, 300000);
  });
}

testWebSocket().catch(console.error);
