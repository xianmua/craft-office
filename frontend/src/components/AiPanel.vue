<template>
  <aside
    v-if="visible"
    class="absolute inset-y-0 right-0 z-20 w-[400px] max-w-[92%] bg-white border-l border-gray-200 flex flex-col"
  >
    <header class="h-12 shrink-0 flex items-center gap-2 px-4 border-b border-gray-100">
      <span class="font-medium text-sm">AI 助手</span>
      <span class="text-xs text-muted">DeepSeek · 密钥在服务端</span>
      <div class="flex-1" />
      <t-button size="small" variant="text" @click="emit('update:visible', false)">关闭</t-button>
    </header>

    <div class="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-4">
      <p v-if="messages.length === 0" class="text-sm text-muted leading-6 m-0">
        {{ docContext ? '文档内容已作为上下文加载，可以直接提问。' : '未加载文档，仅作通用对话。' }}
      </p>
      <div v-for="(m, i) in messages" :key="i" class="flex flex-col gap-1">
        <span class="text-xs text-muted select-none">{{ m.role === 'user' ? '我' : 'DeepSeek' }}</span>
        <div class="text-sm leading-6 whitespace-pre-wrap break-words m-0">
          {{ m.content }}<span v-if="m.loading" class="animate-pulse">▍</span>
          <span v-if="m.error" class="text-red-500">{{ m.content }}</span>
        </div>
      </div>
    </div>

    <footer class="shrink-0 p-3 border-t border-gray-100 flex items-end gap-2">
      <t-textarea
        v-model="input"
        :autosize="{ minRows: 1, maxRows: 4 }"
        placeholder="询问文档内容…（Enter 发送）"
        @enter="send"
      />
      <t-button theme="primary" :disabled="!input.trim() || busy" @click="send">发送</t-button>
    </footer>
  </aside>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'

const props = defineProps<{
  visible: boolean
  docContext: string
}>()

const emit = defineEmits<{
  (e: 'update:visible', value: boolean): void
}>()

interface ChatMsg {
  role: 'user' | 'assistant'
  content: string
  loading?: boolean
  error?: boolean
}

const messages = ref<ChatMsg[]>([])
const input = ref('')
const busy = ref(false)

async function send() {
  const text = input.value.trim()
  if (!text || busy.value) return

  input.value = ''
  busy.value = true
  messages.value.push({ role: 'user', content: text })
  const reply: ChatMsg = { role: 'assistant', content: '', loading: true }
  messages.value.push(reply)

  // 组装对话：文档上下文 + 历史 + 本轮提问
  const history = messages.value
    .filter((m) => !m.loading && !m.error)
    .slice(0, -1)
    .map((m) => ({ role: m.role, content: m.content }))
  const payloadMessages: Array<{ role: string; content: string }> = []
  if (props.docContext) {
    payloadMessages.push({
      role: 'system',
      content: `你是 Office 文档助手。以下是当前文档的纯文本内容，请基于它回答用户问题；内容可能被截断，缺失处请说明。\n\n---\n${props.docContext}\n---`,
    })
  }
  payloadMessages.push(...history)

  try {
    const res = await fetch('/api/ai/chat', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ messages: payloadMessages, stream: true }),
    })
    if (!res.ok) {
      const body = await res.json().catch(() => null)
      throw new Error(body?.message ?? `请求失败（HTTP ${res.status}）`)
    }
    if (!res.body) throw new Error('浏览器不支持流式读取')

    const reader = res.body.getReader()
    const decoder = new TextDecoder()
    let buffer = ''
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      buffer += decoder.decode(value, { stream: true })
      const lines = buffer.split('\n')
      buffer = lines.pop() ?? ''
      for (const line of lines) {
        const trimmed = line.trim()
        if (!trimmed.startsWith('data:')) continue
        const data = trimmed.slice(5).trim()
        if (data === '[DONE]') continue
        try {
          const delta = JSON.parse(data)?.choices?.[0]?.delta?.content
          if (typeof delta === 'string' && delta) reply.content += delta
        } catch {
          /* 忽略不完整行 */
        }
      }
    }
    if (!reply.content) reply.content = '（DeepSeek 未返回内容）'
  } catch (err) {
    reply.error = true
    reply.content = err instanceof Error ? err.message : 'AI 请求失败'
    MessagePlugin.error(reply.content)
  } finally {
    reply.loading = false
    busy.value = false
  }
}
</script>
