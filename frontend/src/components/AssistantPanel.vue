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
    <div ref="listRef" class="flex flex-col gap-3 p-4">
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
          class="inline-block text-left px-3 py-2 rounded-lg max-w-[92%]"
          :class="m.role === 'user' ? 'bg-blue-50' : 'bg-gray-100'"
        >
          <!-- 思考折叠组：收起只显示「思考」二字，点开看完整推理 -->
          <details v-if="m.thinking" class="think mb-1">
            <summary>思考</summary>
            <div class="text-xs text-gray-400 leading-5 mt-1 whitespace-pre-wrap max-h-40 overflow-y-auto">{{ m.thinking }}</div>
          </details>
          {{ m.text || (m.activity ? '' : '…') }}
          <!-- 运行中动作小字 -->
          <span v-if="m.activity" class="flex items-center gap-1.5 text-xs text-gray-400 leading-5">
            <span class="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse"></span>{{ m.activity }}
          </span>
        </span>
      </div>
      <p v-if="error" class="text-sm text-red-500 m-0">{{ error }}</p>
    </div>

    <template #footer>
      <div class="mx-3 mb-3 rounded-xl border border-gray-300 bg-gray-50 focus-within:border-gray-500 p-2 transition-colors">
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
          >{{ aiEngine === 'deepseek' ? 'LLM' : 'DSH' }} ▾</button>
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
import { converse, replySummary, type KernelBridge, type ShellKernel } from '@/artcraft/shellAssistant'
import { aiEngine, setAiEngine } from '@/artcraft/aiEngine'

const props = defineProps<{ visible: boolean; kernel: ShellKernel; bridge: KernelBridge | null }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void; (e: 'busy', v: boolean): void }>()

/** 按内核显示示例指令 */
const HINTS: Record<ShellKernel, string> = {
  wordcraft: '试试：「在文档末尾加一句…」「把第一段加粗」「把所有『旧词』替换成『新词』」。',
  gridcraft: '试试：「把 B2 改成 9999」「按收入从高到低排序」「给表头加粗并填充底色」。',
  deckcraft: '试试：「把第 1 页标题改成…」「新增一页标题加两个要点」「给标题加粗」。',
}
const hints = computed(() => HINTS[props.kernel])

interface Msg {
  role: 'user' | 'assistant'
  text: string
  thinking?: string // 模型推理思考（折叠展示）
  activity?: string // 运行中当前小动作（思考中 / 执行命令）
}
const messages = ref<Msg[]>([])
const history = ref<ChatMessage[]>([]) // OpenAI 兼容历史（不含 system）
const input = ref('')
const busy = ref(false)
const error = ref('')

/** busy 同步给竖条呼吸点 */
watch(busy, (v) => emit('busy', v))

/** 引擎下拉：点外部自动收起 */
const ENGINE_OPTIONS: Array<{ value: AiEngine; label: string; desc: string }> = [
  { value: 'deepseek', label: 'LLM AI', desc: 'DeepSeek 直连' },
  { value: 'dsh', label: 'DSH AI', desc: '本机 DeepSeek Harness' },
]
const engineMenu = ref(false)
function pickEngine(v: AiEngine) {
  setAiEngine(v)
  engineMenu.value = false
}
function onDocClick(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.engine-dd')) engineMenu.value = false
}
onMounted(() => document.addEventListener('click', onDocClick))
onBeforeUnmount(() => document.removeEventListener('click', onDocClick))

const listRef = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()
let ctrl: AbortController | null = null

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
  messages.value.push({ role: 'user', text })
  const live = messages.value.push({ role: 'assistant', text: '', activity: '' }) - 1
  void scrollBottom()

  ctrl = new AbortController()
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
      if (messages.value[live]) messages.value[live].activity = ''
      busy.value = false
      ctrl = null
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
</style>
