import { useState, useCallback, useRef } from 'react'
import type {
  NewSessionRequest,
  PromptRequest,
  SessionNotification,
  ContentBlock
} from '@agentclientprotocol/sdk'
import type { ConnectionStatus, RawMessage, ChatMessage } from '../types'

const WS_URL = 'ws://127.0.0.1:3000/ws'

let requestId = 1

function createJsonRpcRequest(method: string, params: unknown) {
  return {
    jsonrpc: '2.0',
    id: requestId++,
    method,
    params,
  }
}

type StreamingRole = 'assistant' | 'thought' | null

export function useWebSocket() {
  const [status, setStatus] = useState<ConnectionStatus>('disconnected')
  const [sessionId, setSessionId] = useState<string | null>(null)
  const [rawMessages, setRawMessages] = useState<RawMessage[]>([])
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([])
  const wsRef = useRef<WebSocket | null>(null)
  const streamingRoleRef = useRef<StreamingRole>(null)
  const streamingMessageIdRef = useRef<string | null>(null)

  const addRawMessage = useCallback((direction: 'sent' | 'received', data: unknown) => {
    setRawMessages(prev => [...prev, {
      id: crypto.randomUUID(),
      timestamp: new Date(),
      direction,
      data,
    }])
  }, [])

  const addChatMessage = useCallback((role: ChatMessage['role'], content: string) => {
    const id = crypto.randomUUID()
    setChatMessages(prev => [...prev, {
      id,
      role,
      content,
      timestamp: new Date(),
    }])
    return id
  }, [])

  const appendToStreamingMessage = useCallback((role: 'assistant' | 'thought', text: string) => {
    // If role changed or no streaming message, start a new one
    if (streamingRoleRef.current !== role || !streamingMessageIdRef.current) {
      streamingRoleRef.current = role
      const id = crypto.randomUUID()
      streamingMessageIdRef.current = id
      setChatMessages(prev => [...prev, {
        id,
        role,
        content: text,
        timestamp: new Date(),
      }])
    } else {
      // Append to existing message
      setChatMessages(prev => prev.map(msg =>
        msg.id === streamingMessageIdRef.current
          ? { ...msg, content: msg.content + text }
          : msg
      ))
    }
  }, [])

  const endStreaming = useCallback(() => {
    streamingRoleRef.current = null
    streamingMessageIdRef.current = null
  }, [])

  const connect = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return

    setStatus('connecting')
    const ws = new WebSocket(WS_URL)
    wsRef.current = ws

    ws.onopen = () => {
      setStatus('connected')
      addChatMessage('system', 'WebSocket connected')
    }

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data)
        addRawMessage('received', data)

        // Handle JSON-RPC response
        if (data.result) {
          if (data.result.sessionId) {
            setSessionId(data.result.sessionId)
            addChatMessage('system', `Session created: ${data.result.sessionId}`)
          }
          if (data.result.stopReason) {
            // Prompt completed, end streaming
            endStreaming()
          }
        } else if (data.error) {
          addChatMessage('system', `Error: ${data.error.message}`)
          endStreaming()
        } else if (data.method) {
          // Handle notification
          handleNotification(data.method, data.params)
        }
      } catch {
        addRawMessage('received', event.data)
      }
    }

    ws.onerror = () => {
      setStatus('error')
      addChatMessage('system', 'WebSocket error')
      endStreaming()
    }

    ws.onclose = () => {
      setStatus('disconnected')
      setSessionId(null)
      addChatMessage('system', 'WebSocket disconnected')
      endStreaming()
    }
  }, [addRawMessage, addChatMessage, endStreaming])

  const handleNotification = useCallback((method: string, params: SessionNotification) => {
    if (method === 'session/update' && params) {
      const update = params.update
      // Extract text content from agent message chunks
      if (update.sessionUpdate === 'agent_message_chunk') {
        const content = update.content as ContentBlock
        if (content.type === 'text') {
          appendToStreamingMessage('assistant', content.text)
        }
      }
      // Extract text content from agent thought chunks
      if (update.sessionUpdate === 'agent_thought_chunk') {
        const content = update.content as ContentBlock
        if (content.type === 'text') {
          appendToStreamingMessage('thought', content.text)
        }
      }
    }
  }, [appendToStreamingMessage])

  const disconnect = useCallback(() => {
    wsRef.current?.close()
    wsRef.current = null
  }, [])

  const createSession = useCallback(() => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN) return

    const request = createJsonRpcRequest('session/new', {
      cwd: '/tmp',
      mcpServers: [],
    } satisfies NewSessionRequest)

    addRawMessage('sent', request)
    wsRef.current.send(JSON.stringify(request))
  }, [addRawMessage])

  const sendPrompt = useCallback((text: string) => {
    if (!wsRef.current || wsRef.current.readyState !== WebSocket.OPEN || !sessionId) return

    addChatMessage('user', text)
    endStreaming() // Reset streaming state for new prompt

    const request = createJsonRpcRequest('session/prompt', {
      sessionId,
      prompt: [{ type: 'text', text }],
    } satisfies PromptRequest)

    addRawMessage('sent', request)
    wsRef.current.send(JSON.stringify(request))
  }, [sessionId, addRawMessage, addChatMessage, endStreaming])

  const clearMessages = useCallback(() => {
    setRawMessages([])
    setChatMessages([])
    endStreaming()
  }, [endStreaming])

  return {
    status,
    sessionId,
    rawMessages,
    chatMessages,
    connect,
    disconnect,
    createSession,
    sendPrompt,
    clearMessages,
  }
}
