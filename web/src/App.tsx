import * as Separator from '@radix-ui/react-separator'
import { ChatPanel } from './components/ChatPanel'
import { DebugPanel } from './components/DebugPanel'
import { useWebSocket } from './hooks/useWebSocket'

export default function App() {
  const {
    status,
    sessionId,
    rawMessages,
    chatMessages,
    connect,
    disconnect,
    createSession,
    sendPrompt,
    clearMessages,
  } = useWebSocket()

  return (
    <div className="h-screen flex">
      {/* Left Panel - Chat */}
      <div className="w-1/2 min-w-0 border-r border-gray-700">
        <ChatPanel
          status={status}
          sessionId={sessionId}
          messages={chatMessages}
          onConnect={connect}
          onDisconnect={disconnect}
          onCreateSession={createSession}
          onSendPrompt={sendPrompt}
        />
      </div>

      <Separator.Root
        className="w-px bg-gray-700 flex-shrink-0"
        orientation="vertical"
      />

      {/* Right Panel - Debug */}
      <div className="w-1/2 min-w-0">
        <DebugPanel messages={rawMessages} onClear={clearMessages} />
      </div>
    </div>
  )
}
