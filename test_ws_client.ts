import WebSocket from "ws";

const uri = "ws://127.0.0.1:3000/ws";

async function testWebSocket() {
  return new Promise<void>((resolve, reject) => {
    const ws = new WebSocket(uri);

    ws.on("open", () => {
      // 发送 NewSessionRequest
      const request = {
        type: "new_session",
        cwd: "/tmp",
        mcpServers: [],
      };
      ws.send(JSON.stringify(request));
      console.log(`Sent: ${JSON.stringify(request)}`);
    });

    ws.on("message", (data) => {
      const response = data.toString();
      console.log(`Received: ${response}`);

      // 解析响应
      const responseData = JSON.parse(response);
      if (responseData.type === "new_session") {
        console.log(`✅ Success! Session ID: ${responseData.sessionId}`);
      } else {
        console.log(`❌ Unexpected response type: ${responseData.type}`);
      }

      ws.close();
      resolve();
    });

    ws.on("error", (err) => {
      console.log(`❌ Error: ${err.message}`);
      reject(err);
    });

    ws.on("close", () => {
      resolve();
    });
  });
}

testWebSocket().catch(console.error);
