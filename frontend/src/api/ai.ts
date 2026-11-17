/**
 * AI 对话代理前端封装：密钥只在后端（见 backend/src/routes/ai.rs），前端只发消息。
 * 流式响应为 OpenAI 兼容 SSE（data: {choices:[{delta:{content}}]}，data: [DONE] 结束）。
 */

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

import { apiUrl } from './base'

/** 流式对话：onDelta 逐段回调增量文本，resolve 为完整回复；signal 用于取消（关闭即取消） */
export async function chatStream(messages: ChatMessage[], onDelta: (delta: string) => void, signal?: AbortSignal): Promise<string> {
  const res = await fetch(apiUrl('ai/chat'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ messages, stream: true }),
    signal,
  })
  if (!res.ok || !res.body) {
    let detail = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body?.message) detail = body.message
    } catch {
      /* 非 JSON 错误体 */
    }
    throw new Error(detail)
  }

  const reader = res.body.getReader()
  const decoder = new TextDecoder()
  let full = ''
  let buffer = ''

  for (;;) {
    const { done, value } = await reader.read()
    if (done) break
    buffer += decoder.decode(value, { stream: true })
    // SSE 事件以空行分隔，逐行取 data: 载荷
    let nl: number
    while ((nl = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, nl).trim()
      buffer = buffer.slice(nl + 1)
      if (!line.startsWith('data:')) continue
      const payload = line.slice(5).trim()
      if (payload === '[DONE]') return full
      try {
        const chunk = JSON.parse(payload)
        const delta: string | undefined = chunk?.choices?.[0]?.delta?.content
        if (delta) {
          full += delta
          onDelta(delta)
        }
      } catch {
        /* 忽略残缺分片 */
      }
    }
  }
  return full
}
