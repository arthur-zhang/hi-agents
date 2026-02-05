import { useState } from 'react'
import * as ScrollArea from '@radix-ui/react-scroll-area'
import * as Collapsible from '@radix-ui/react-collapsible'
import type { RawMessage } from '../types'

interface DebugPanelProps {
  messages: RawMessage[]
  onClear: () => void
}

export function DebugPanel({ messages, onClear }: DebugPanelProps) {
  return (
    <div className="flex flex-col h-full bg-gray-950">
      {/* Header */}
      <div className="p-4 border-b border-gray-800 flex items-center justify-between">
        <h2 className="font-semibold text-gray-300">WebSocket Debug</h2>
        <button
          onClick={onClear}
          className="px-3 py-1 text-sm bg-gray-800 hover:bg-gray-700 rounded"
        >
          Clear
        </button>
      </div>

      {/* Messages */}
      <ScrollArea.Root className="flex-1 overflow-hidden">
        <ScrollArea.Viewport className="h-full w-full p-2">
          <div className="space-y-1">
            {messages.map((msg) => (
              <MessageItem key={msg.id} message={msg} />
            ))}
          </div>
        </ScrollArea.Viewport>
        <ScrollArea.Scrollbar
          className="flex select-none touch-none p-0.5 bg-gray-900 transition-colors duration-150 ease-out hover:bg-gray-800 data-[orientation=vertical]:w-2.5"
          orientation="vertical"
        >
          <ScrollArea.Thumb className="flex-1 bg-gray-700 rounded-full relative" />
        </ScrollArea.Scrollbar>
      </ScrollArea.Root>
    </div>
  )
}

function MessageItem({ message }: { message: RawMessage }) {
  const [open, setOpen] = useState(false)
  const isSent = message.direction === 'sent'

  return (
    <Collapsible.Root open={open} onOpenChange={setOpen}>
      <Collapsible.Trigger asChild>
        <button
          className={`w-full text-left p-2 rounded text-sm font-mono ${
            isSent
              ? 'bg-green-900/30 hover:bg-green-900/50 border-l-2 border-green-500'
              : 'bg-blue-900/30 hover:bg-blue-900/50 border-l-2 border-blue-500'
          }`}
        >
          <div className="flex items-center gap-2">
            <span className={isSent ? 'text-green-400' : 'text-blue-400'}>
              {isSent ? '↑' : '↓'}
            </span>
            <span className="text-gray-500 text-xs">
              {message.timestamp.toLocaleTimeString()}
            </span>
            <span className="text-gray-400 truncate flex-1">
              {getMessagePreview(message.data)}
            </span>
            <span className="text-gray-600">{open ? '▼' : '▶'}</span>
          </div>
        </button>
      </Collapsible.Trigger>
      <Collapsible.Content>
        <pre className="p-3 bg-gray-900 rounded-b text-xs overflow-x-auto text-gray-300">
          {JSON.stringify(message.data, null, 2)}
        </pre>
      </Collapsible.Content>
    </Collapsible.Root>
  )
}

function getMessagePreview(data: unknown): string {
  if (typeof data === 'object' && data !== null) {
    const obj = data as Record<string, unknown>
    if (obj.method) return `${obj.method}`
    if (obj.result) return 'result'
    if (obj.error) return `error: ${(obj.error as Record<string, unknown>).message}`
  }
  return String(data).slice(0, 50)
}