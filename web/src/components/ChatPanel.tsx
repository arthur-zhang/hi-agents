import { useState, KeyboardEvent } from 'react'
import * as ScrollArea from '@radix-ui/react-scroll-area'
import * as Separator from '@radix-ui/react-separator'
import type { ConnectionStatus, ChatMessage } from '../types'

interface ChatPanelProps {
  status: ConnectionStatus
  sessionId: string | null
  messages: ChatMessage[]
  onConnect: () => void
  onDisconnect: () => void
  onCreateSession: () => void
  onSendPrompt: (text: string) => void
}

export function ChatPanel({
  status,
  sessionId,
  messages,
  onConnect,
  onDisconnect,
  onCreateSession,
  onSendPrompt,
}: ChatPanelProps) {
  const [input, setInput] = useState('')

  const handleSend = () => {
    if (!input.trim() || !sessionId) return
    onSendPrompt(input.trim())
    setInput('')
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      handleSend()
    }
  }

  const statusColor = {
    disconnected: 'bg-gray-500',
    connecting: 'bg-yellow-500',
    connected: 'bg-green-500',
    error: 'bg-red-500',
  }[status]

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="p-4 border-b border-gray-700">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <div className={`w-3 h-3 rounded-full ${statusColor}`} />
            <span className="text-sm text-gray-400 capitalize">{status}</span>
          </div>
          {sessionId && (
            <span className="text-xs text-gray-500 font-mono truncate max-w-[200px]">
              {sessionId}
            </span>
          )}
        </div>
        <div className="flex gap-2">
          {status === 'disconnected' ? (
            <button
              onClick={onConnect}
              className="px-3 py-1.5 bg-blue-600 hover:bg-blue-700 rounded text-sm"
            >
              Connect
            </button>
          ) : (
            <button
              onClick={onDisconnect}
              className="px-3 py-1.5 bg-gray-600 hover:bg-gray-700 rounded text-sm"
            >
              Disconnect
            </button>
          )}
          <button
            onClick={onCreateSession}
            disabled={status !== 'connected' || !!sessionId}
            className="px-3 py-1.5 bg-green-600 hover:bg-green-700 disabled:bg-gray-600 disabled:cursor-not-allowed rounded text-sm"
          >
            Create Session
          </button>
        </div>
      </div>

      <Separator.Root className="h-px bg-gray-700" />

      {/* Messages */}
      <ScrollArea.Root className="flex-1 overflow-hidden">
        <ScrollArea.Viewport className="h-full w-full p-4">
          <div className="space-y-3">
            {messages.map((msg) => (
              <div
                key={msg.id}
                className={`p-3 rounded-lg ${
                  msg.role === 'user'
                    ? 'bg-blue-900/50 ml-8'
                    : msg.role === 'assistant'
                    ? 'bg-gray-800 mr-8'
                    : msg.role === 'thought'
                    ? 'bg-purple-900/30 mr-8 border-l-2 border-purple-500 italic'
                    : 'bg-gray-700/50 text-gray-400 text-sm'
                }`}
              >
                <div className="text-xs text-gray-500 mb-1">
                  {msg.role === 'thought' ? 'thinking' : msg.role} · {msg.timestamp.toLocaleTimeString()}
                </div>
                <div className="whitespace-pre-wrap">{msg.content}</div>
              </div>
            ))}
          </div>
        </ScrollArea.Viewport>
        <ScrollArea.Scrollbar
          className="flex select-none touch-none p-0.5 bg-gray-800 transition-colors duration-150 ease-out hover:bg-gray-700 data-[orientation=vertical]:w-2.5 data-[orientation=horizontal]:flex-col data-[orientation=horizontal]:h-2.5"
          orientation="vertical"
        >
          <ScrollArea.Thumb className="flex-1 bg-gray-600 rounded-full relative" />
        </ScrollArea.Scrollbar>
      </ScrollArea.Root>

      <Separator.Root className="h-px bg-gray-700" />

      {/* Input */}
      <div className="p-4">
        <div className="flex gap-2">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={sessionId ? 'Type a message...' : 'Create a session first'}
            disabled={!sessionId}
            className="flex-1 bg-gray-800 border border-gray-700 rounded-lg p-3 resize-none h-20 focus:outline-none focus:border-blue-500 disabled:opacity-50"
          />
          <button
            onClick={handleSend}
            disabled={!sessionId || !input.trim()}
            className="px-4 bg-blue-600 hover:bg-blue-700 disabled:bg-gray-600 disabled:cursor-not-allowed rounded-lg"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  )
}