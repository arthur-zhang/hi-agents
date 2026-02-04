#!/usr/bin/env python3
import asyncio
import websockets
import json

async def test_websocket():
    uri = "ws://127.0.0.1:3000/ws"

    try:
        async with websockets.connect(uri) as websocket:
            # 发送 NewSessionRequest
            request = {
                "type": "new_session",
                "cwd": "/tmp",
                "mcpServers": []
            }
            await websocket.send(json.dumps(request))
            print(f"Sent: {json.dumps(request)}")

            # 接收响应
            response = await websocket.recv()
            print(f"Received: {response}")

            # 解析响应
            response_data = json.loads(response)
            if response_data.get("type") == "new_session":
                print(f"✅ Success! Session ID: {response_data.get('sessionId')}")
            else:
                print(f"❌ Unexpected response type: {response_data.get('type')}")

    except Exception as e:
        print(f"❌ Error: {e}")

if __name__ == "__main__":
    asyncio.run(test_websocket())
