/**
 * AI 对话代理前端封装：密钥只在后端（见 backend/src/routes/ai.rs），前端只发消息。
 * 流式响应为 OpenAI 兼容 SSE（data: {choices:[{delta:{content}}]}，data: [DONE] 结束）。
 */

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant'
  content: string
}

import { apiUrl } from './base'

/** AI 引擎 id：内置 deepseek + 插件引擎（plugins 目录下 engine.json 声明，如 dsh） */
export type AiEngine = string

/** 流式对话：onDelta 逐段回调增量正文，onReason 回调推理思考增量（混合推理模型才有）；
 * resolve 为完整回复；signal 用于取消（关闭即取消） */
export async function chatStream(
  messages: ChatMessage[],
  onDelta: (delta: string) => void,
  signal?: AbortSignal,
  engine: AiEngine = 'deepseek',
  onReason?: (delta: string) => void,
): Promise<string> {
  const res = await fetch(apiUrl('ai/chat'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ messages, stream: true, engine }),
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
        const delta = chunk?.choices?.[0]?.delta
        const reason: string | undefined = delta?.reasoning_content
        if (reason && onReason) onReason(reason)
        const content: string | undefined = delta?.content
        if (content) {
          full += content
          onDelta(content)
        }
      } catch {
        /* 忽略残缺分片 */
      }
    }
  }
  return full
}

// ===== 设置面板：引擎配置读写（key 只回脱敏掩码）=====

/** 引擎视图：meta（插件 engine.json 声明）+ 当前配置（未配置时后三项缺省） */
export interface EngineConfigView {
  id: AiEngine
  label: string
  desc: string
  keyRequired: boolean
  defaultBaseUrl: string
  defaultModel: string
  configured: boolean
  apiKeyMasked?: string
  baseUrl?: string
  model?: string
}

export type AiConfigView = EngineConfigView[]

export async function fetchAiConfig(): Promise<AiConfigView> {
  const res = await fetch(apiUrl('ai/config'))
  if (!res.ok) throw new Error(`读取 AI 配置失败（HTTP ${res.status}）`)
  return res.json()
}

/** 更新引擎配置；apiKey 不传 = 保持不变。即时生效并持久化到后端 .env */
export async function updateAiConfig(
  engine: AiEngine,
  patch: { apiKey?: string; baseUrl?: string; model?: string },
): Promise<{ apiKeyMasked: string; baseUrl: string; model: string }> {
  const res = await fetch(apiUrl('ai/config'), {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ engine, ...patch }),
  })
  if (!res.ok) {
    let detail = `HTTP ${res.status}`
    try {
      const body = await res.json()
      if (body?.message) detail = body.message
    } catch {
      /* 非 JSON 错误体 */
    }
    throw new Error(detail)
  }
  return res.json()
}
