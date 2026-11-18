<template>
  <!-- AI 助手面板：通用 SidePanel 容器 + 对话内容；消息状态在组件内，面板隐藏不丢 -->
  <SidePanel
    :visible="visible"
    title="AI 助手"
    :busy="busy"
    close-title="关闭（运行中的任务会被取消）"
    @minimize="emit('update:visible', false)"
    @close="close"
  >
    <!-- 头部动作区：会话列表 / 返回对话 切换 -->
    <template #actions>
      <button
        class="w-6 h-6 flex items-center justify-center border-0 bg-transparent p-0 text-gray-400 hover:text-gray-600 cursor-pointer"
        :title="view === 'chat' ? '会话列表' : '返回对话'"
        @click="view = view === 'chat' ? 'sessions' : 'chat'"
      >
        <svg v-if="view === 'chat'" class="w-4 h-4" viewBox="0 0 16 16" fill="none">
          <path d="M3 4h10M3 8h10M3 12h7" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
        <svg v-else class="w-4 h-4" viewBox="0 0 16 16" fill="none">
          <path d="M13 3v10M9.5 4.5L6 8l3.5 3.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    </template>

    <!-- 会话列表视图 -->
    <div v-if="view === 'sessions'" class="flex flex-col p-3">
      <button
        class="mb-2 self-start border-0 bg-transparent p-0 text-xs text-blue-600 hover:text-blue-700 cursor-pointer"
        :disabled="busy"
        title="开始新会话"
        @click="newSession"
      >＋ 新会话</button>
      <p v-if="sessions.length === 0" class="text-sm text-muted m-0">暂无历史会话</p>
      <div
        v-for="s in sessions"
        :key="s.id"
        class="session-item px-3 py-2 rounded-lg cursor-pointer hover:bg-gray-50 border-b border-gray-100 last:border-b-0"
        :class="{ 'opacity-50 pointer-events-none': busy }"
        @click="loadSession(s.id)"
      >
        <div class="flex items-center justify-between gap-2">
          <span class="text-sm text-gray-700 truncate">{{ sessionTitle(s) }}</span>
          <span class="flex items-center gap-1.5 shrink-0">
            <span class="text-xs text-gray-400">{{ sessionDate(s.createdAt) }}</span>
            <!-- 删除会话：两段式二次确认（首点变红，3 秒内再点执行） -->
            <span
              class="w-4 text-center cursor-pointer select-none"
              :class="confirmDel === s.id ? 'text-red-500 font-semibold' : 'text-gray-300 hover:text-red-400'"
              :title="confirmDel === s.id ? '再点一次确认删除' : '删除会话'"
              @click.stop="removeSession(s.id)"
            >{{ confirmDel === s.id ? '✓' : '×' }}</span>
          </span>
        </div>
        <div class="text-xs text-gray-400 mt-0.5 truncate">{{ s.id }}</div>
      </div>
    </div>

    <!-- 对话视图 -->
    <div v-show="view === 'chat'" ref="listRef" class="flex flex-col gap-3 p-4">
      <p v-if="messages.length === 0" class="text-sm text-muted m-0 leading-6">
        {{ hints }}
      </p>
      <div
        v-for="(m, i) in messages"
        :key="i"
        class="text-sm leading-6 whitespace-pre-wrap break-words"
        :class="m.role === 'user' ? 'text-right' : 'text-left'"
      >
        <span
          class="inline-block text-left px-3 py-2 rounded-lg w-[95%]"
          :class="m.role === 'user' ? 'bg-blue-50' : 'bg-gray-100'"
        >
          <!-- 思考过程折叠组：位于回复上方；运行中实时逐行展示（无标题），完成后折叠为「已思考」 -->
          <details v-if="m.thinking" class="think mb-1" :open="busy && i === messages.length - 1">
            <summary v-if="!(busy && i === messages.length - 1)">已思考</summary>
            <div
              :ref="busy && i === messages.length - 1 ? setThinkRef : undefined"
              class="text-xs text-gray-400 leading-5 mt-1 max-h-40 overflow-y-auto"
            >
              <div v-for="(line, li) in thinkLines(m.thinking)" :key="li">{{ line }}</div>
            </div>
          </details>
          {{ m.text || (m.activity ? '' : '…') }}
          <!-- 运行中动作小字 -->
          <span v-if="m.activity" class="flex items-center gap-1.5 text-xs text-gray-400 leading-5">
            <span class="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse"></span>{{ m.activity }}
          </span>
        </span>
        <!-- 回复用时：完成后气泡左侧小字 -->
        <div v-if="m.elapsed" class="text-[11px] text-gray-300 mt-0.5">{{ m.elapsed }}s</div>
      </div>
      <p v-if="error" class="text-sm text-red-500 m-0">{{ error }}</p>
    </div>

    <template #footer>
      <div v-if="view === 'chat'" class="mx-3 mb-3 rounded-xl border border-gray-300 bg-gray-50 focus-within:border-gray-500 p-2 transition-colors">
        <textarea
          ref="inputRef"
          v-model="input"
          rows="1"
          class="block w-full bg-transparent border-0 outline-none resize-none text-sm leading-6 placeholder-gray-400 max-h-[120px] overflow-y-auto"
          placeholder="发消息，回车发送"
          @input="autoSize"
          @keydown.enter.exact.prevent="onSend"
        ></textarea>
        <div class="flex items-center justify-end gap-2 pt-1">
          <div class="engine-dd relative">
          <button
            class="h-6 px-2 rounded-full border-0 p-0 cursor-pointer text-xs leading-none bg-transparent text-gray-400 hover:text-gray-600 transition-colors"
            title="选择 AI 引擎"
            @click.stop="engineMenu = !engineMenu"
            >{{ engineLabel }} ▾</button>
          <!-- 引擎下拉：LLM AI / DSH AI -->
          <div
            v-if="engineMenu"
            class="absolute bottom-8 right-0 z-10 w-44 bg-white border border-gray-200 rounded-lg shadow-lg py-1"
          >
            <button
              v-for="opt in ENGINE_OPTIONS"
              :key="opt.value"
              class="w-full flex flex-col items-start gap-0.5 border-0 bg-transparent p-0 px-3 py-2 text-left cursor-pointer transition-colors hover:bg-gray-50"
              @click.stop="pickEngine(opt.value)"
            >
              <span class="text-xs font-medium leading-4" :class="aiEngine === opt.value ? 'text-amber-600' : 'text-gray-700'">
                {{ opt.label }}{{ aiEngine === opt.value ? ' ✓' : '' }}
              </span>
              <span class="text-[11px] text-gray-400 leading-4">{{ opt.desc }}</span>
            </button>
          </div>
        </div>
          <button
            class="w-7 h-7 rounded-full flex items-center justify-center border-0 p-0 cursor-pointer transition-colors"
            :class="input.trim() && !busy ? 'bg-blue-600 text-white hover:bg-blue-700' : 'bg-gray-200 text-gray-400 cursor-default'"
            title="发送"
            @click="onSend"
          >
            <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
              <path d="M8 13V3M3.5 7.5L8 3l4.5 4.5" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        </div>
      </div>
    </template>
  </SidePanel>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import SidePanel from '@/components/SidePanel.vue'
import type { AiEngine, ChatMessage } from '@/api/ai'
import { converse, replySummary, kernelDesc, type KernelBridge, type ShellKernel } from '@/artcraft/shellAssistant'
import { aiEngine, aiEngines, ensureEngines, setAiEngine } from '@/artcraft/aiEngine'

const props = defineProps<{ visible: boolean; kernel: ShellKernel; bridge: KernelBridge | null }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void; (e: 'busy', v: boolean): void }>()

/** 示例指令：来自内核注册表（kernels.json hints 字段） */
const hints = computed(() => kernelDesc(props.kernel)?.hints ?? '')

interface Msg {
  role: 'user' | 'assistant'
  text: string
  thinking?: string // 模型推理思考（运行中逐行实时展示，完成后折叠）
  activity?: string // 运行中当前小动作（思考中 / 执行命令）
  elapsed?: number // 本轮回复用时（秒，一位小数）
}
const messages = ref<Msg[]>([])
const history = ref<ChatMessage[]>([]) // OpenAI 兼容历史（不含 system）
const input = ref('')
const busy = ref(false)
const error = ref('')

// ---- 会话持久化：localStorage 存历史会话，列表可回载 ----
interface StoredSession {
  id: string
  createdAt: number
  messages: Msg[]
  history: ChatMessage[]
}
const SESSIONS_KEY = 'artcraft-ai-sessions'
const SESSIONS_MAX = 30
const view = ref<'chat' | 'sessions'>('chat')
const sid = ref('') // 当前会话 id（空 = 尚未落库，首次发消息时创建）
const sessions = ref<StoredSession[]>([])

function loadSessions(): StoredSession[] {
  try {
    const list = JSON.parse(localStorage.getItem(SESSIONS_KEY) || '[]') as StoredSession[]
    return Array.isArray(list) ? list : []
  } catch {
    return []
  }
}
function refreshSessions() {
  sessions.value = loadSessions().sort((a, b) => b.createdAt - a.createdAt)
}
/** 把当前对话写入存储（对话一轮结束或切会话前调用） */
function persistSession() {
  if (!sid.value || messages.value.length === 0) return
  const list = loadSessions().filter((s) => s.id !== sid.value)
  list.push({
    id: sid.value,
    createdAt: sessions.value.find((s) => s.id === sid.value)?.createdAt ?? Date.now(),
    messages: JSON.parse(JSON.stringify(messages.value)),
    history: JSON.parse(JSON.stringify(history.value)),
  })
  list.sort((a, b) => b.createdAt - a.createdAt)
  try {
    localStorage.setItem(SESSIONS_KEY, JSON.stringify(list.slice(0, SESSIONS_MAX)))
  } catch { /* 存储满则放弃本轮落库 */ }
  refreshSessions()
}
function sessionTitle(s: StoredSession): string {
  const first = s.messages.find((m) => m.role === 'user')?.text ?? ''
  return first.slice(0, 20) || '（空会话）'
}
function sessionDate(t: number): string {
  return new Date(t).toLocaleString('zh-CN', { hour12: false })
}
/** 回载历史会话：替换当前对话内容并回到聊天视图 */
function loadSession(id: string) {
  if (busy.value) return
  persistSession()
  const s = loadSessions().find((x) => x.id === id)
  if (!s) return
  sid.value = s.id
  messages.value = JSON.parse(JSON.stringify(s.messages))
  history.value = JSON.parse(JSON.stringify(s.history))
  error.value = ''
  view.value = 'chat'
  void scrollBottom()
}
/** 开新会话：当前对话先落库，清空现场 */
function newSession() {
  if (busy.value) return
  persistSession()
  sid.value = ''
  messages.value = []
  history.value = []
  error.value = ''
  view.value = 'chat'
}

/** 删除会话：两段式确认（首点标记 3 秒内再点执行）；删当前会话则直接清空现场不回写 */
const confirmDel = ref('')
let delTimer: ReturnType<typeof setTimeout> | undefined
function removeSession(id: string) {
  if (confirmDel.value !== id) {
    confirmDel.value = id
    clearTimeout(delTimer)
    delTimer = setTimeout(() => (confirmDel.value = ''), 3000)
    return
  }
  confirmDel.value = ''
  clearTimeout(delTimer)
  localStorage.setItem(SESSIONS_KEY, JSON.stringify(loadSessions().filter((s) => s.id !== id)))
  if (id === sid.value) {
    sid.value = ''
    messages.value = []
    history.value = []
    view.value = 'chat'
  }
  refreshSessions()
}

/** busy 同步给竖条呼吸点 */
watch(busy, (v) => emit('busy', v))

/** 引擎下拉：列表来自后端引擎注册表（内置 + 插件），点外部自动收起 */
const ENGINE_OPTIONS = computed(() =>
  aiEngines.value.map((e) => ({ value: e.id, label: e.label, desc: e.desc })),
)
const engineLabel = computed(
  () => aiEngines.value.find((e) => e.id === aiEngine.value)?.label ?? 'LLM',
)
const engineMenu = ref(false)
function pickEngine(v: AiEngine) {
  setAiEngine(v)
  engineMenu.value = false
}
function onDocClick(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.engine-dd')) engineMenu.value = false
}
onMounted(() => {
  document.addEventListener('click', onDocClick)
  void ensureEngines() // 引擎下拉列表（后端注册表）
  refreshSessions() // 历史会话列表
})
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))

const listRef = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()
let ctrl: AbortController | null = null

/** 思考过程逐行拆分（空行丢弃）与运行中自动滚底 */
function thinkLines(t: string): string[] {
  return t.split('\n').map((s) => s.trim()).filter(Boolean)
}
const thinkRef = ref<HTMLElement>()
function setThinkRef(el: unknown) {
  thinkRef.value = el as HTMLElement
}
watch(
  () => messages.value[messages.value.length - 1]?.thinking,
  async () => {
    await nextTick()
    const el = thinkRef.value
    if (el) el.scrollTop = el.scrollHeight
  },
)

async function scrollBottom() {
  await nextTick()
  listRef.value?.scrollTo({ top: listRef.value.scrollHeight })
}

/** 关闭即取消：中止进行中的请求与命令序列 */
function close() {
  ctrl?.abort()
  ctrl = null
  busy.value = false
  emit('update:visible', false)
}

/** 输入框随内容自动增高（上限 120px 后内部滚动） */
function autoSize() {
  const el = inputRef.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, 120)}px`
}

function onSend() {
  const text = input.value.trim()
  if (!text || busy.value || !props.bridge) return
  input.value = ''
  busy.value = true
  error.value = ''
  if (!sid.value) sid.value = `s-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
  messages.value.push({ role: 'user', text })
  const live = messages.value.push({ role: 'assistant', text: '', activity: '' }) - 1
  void scrollBottom()

  ctrl = new AbortController()
  const t0 = Date.now()
  converse({
    kernel: props.kernel,
    bridge: props.bridge,
    history: history.value,
    text,
    signal: ctrl.signal,
    engine: aiEngine.value,
    onDelta: (liveText) => {
      messages.value[live].text = liveText
      void scrollBottom()
    },
    onReason: (full) => {
      messages.value[live].thinking = full
    },
    onActivity: (label) => {
      messages.value[live].activity = label
    },
  })
    .then((reply) => {
      // 命令执行完补最终展示文本（纯操作回复显示摘要，不留白泡）
      const shown = replySummary(reply)
      if (shown) messages.value[live].text = shown
      else messages.value.splice(live, 1)
      if (messages.value[live]) {
        messages.value[live].activity = ''
        messages.value[live].elapsed = Math.round((Date.now() - t0) / 100) / 10
      }
      busy.value = false
      ctrl = null
      persistSession()
      void scrollBottom()
    })
    .catch((e) => {
      // 主动取消不算错误
      if ((e as Error)?.name !== 'AbortError') {
        error.value = String((e && (e as Error).message) || e)
      } else {
        messages.value.splice(live, 1)
      }
      busy.value = false
      ctrl = null
      persistSession()
      void scrollBottom()
    })
}
</script>

<style scoped>
/* 思考折叠组：收起只显示「思考」二字（去默认三角） */
.think summary {
  font-size: 12px;
  color: #9ca3af;
  cursor: pointer;
  user-select: none;
  list-style: none;
}
.think summary::-webkit-details-marker {
  display: none;
}
.think summary:hover {
  color: #6b7280;
}
/* 思考滚动区：细滚动条，悬停才加深 */
.think div::-webkit-scrollbar {
  width: 4px;
}
.think div::-webkit-scrollbar-thumb {
  background: #e5e7eb;
  border-radius: 2px;
}
.think div:hover::-webkit-scrollbar-thumb {
  background: #d1d5db;
}
.think div {
  scrollbar-width: thin;
  scrollbar-color: #e5e7eb transparent;
}
</style>
