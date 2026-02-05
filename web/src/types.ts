import type { SessionNotification } from '@agentclientprotocol/sdk'

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error'

export interface RawMessage {
  id: string
  timestamp: Date
  direction: 'sent' | 'received'
  data: unknown
}

export interface ChatMessage {
  id: string
  role: 'user' | 'assistant' | 'thought' | 'system'
  content: string
  timestamp: Date
}

export type SessionUpdate = SessionNotification
