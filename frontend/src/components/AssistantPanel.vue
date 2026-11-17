<template>
  <!-- 贴边竖条（常驻入口 + 最小化形态）：可上下拖拽，点击展开；任务继续在后台运行 -->
  <div
    v-if="minimized || !show"
    class="fixed right-0 z-30 flex flex-col items-center justify-center gap-1 w-4 h-24 bg-amber-400 border border-r-0 border-amber-500 rounded-l-lg cursor-pointer select-none touch-none"
    :style="{ top: barTop + 'px' }"
    @pointerdown="onBarDown"
    @click="onBarClick"
  >
    <span v-if="busy" class="w-1 h-1 rounded-full bg-white animate-pulse"></span>
    <svg class="w-3 h-3 text-white" viewBox="0 0 16 16" fill="none">
      <path d="M10 3L5 8l5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
    </svg>
  </div>

  <!-- 展开：吸顶面板，可向左 / 向下拉大 -->
  <div
    v-else
    class="fixed top-0 right-4 z-30 flex flex-col bg-white shadow-2xl rounded-b-xl overflow-hidden"
    :style="{ width: panelW + 'px', height: panelH + 'px' }"
  >
    <!-- 拉大手柄：左边缘 / 底边缘 / 左下角 / 右下角 -->
    <div class="absolute left-0 top-0 bottom-0 w-2 cursor-ew-resize z-10" @pointerdown="startResize($event, 'left')"></div>
    <div class="absolute bottom-0 left-0 right-0 h-2 cursor-ns-resize z-10" @pointerdown="startResize($event, 'bottom')"></div>
    <div class="absolute left-0 bottom-0 w-4 h-4 cursor-nesw-resize z-10" @pointerdown="startResize($event, 'corner')"></div>
    <div class="absolute right-0 bottom-0 w-4 h-4 cursor-nwse-resize z-10" @pointerdown="startResize($event, 'corner')">
      <svg class="absolute right-0.5 bottom-0.5 w-2.5 h-2.5 text-gray-300" viewBox="0 0 10 10" fill="none">
        <path d="M9 1L1 9M9 5L5 9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" />
      </svg>
    </div>

    <div class="flex items-center gap-2 pl-4 pr-2 h-11 shrink-0 border-b border-gray-100">
      <span class="text-sm font-medium">AI 助手</span>
      <span v-if="busy" class="text-xs text-gray-400">任务运行中…</span>
      <div class="flex-1"></div>
      <button
        class="w-6 h-6 flex items-center justify-center border-0 bg-transparent p-0 text-gray-400 hover:text-gray-600 cursor-pointer"
        title="最小化"
        @click="minimize"
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
          <path d="M6 3l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <button
        class="w-6 h-6 flex items-center justify-center border-0 bg-transparent p-0 text-gray-400 hover:text-gray-600 cursor-pointer"
        title="关闭（运行中的任务会被取消）"
        @click="close"
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>
    <div ref="listRef" class="flex-1 overflow-y-auto flex flex-col gap-3 p-4">
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
        >{{ m.text || '…' }}</span>
      </div>
      <p v-if="error" class="text-sm text-red-500 m-0">{{ error }}</p>
    </div>
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
      <div class="flex items-center justify-end pt-1">
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
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'
import type { ChatMessage } from '@/api/ai'
import { converse, liveDisplay, type KernelBridge, type ShellKernel } from '@/artcraft/shellAssistant'

const props = defineProps<{ visible: boolean; kernel: ShellKernel; bridge: KernelBridge | null }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

const show = computed({
  get: () => props.visible,
  set: (v: boolean) => emit('update:visible', v),
})

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
}
const messages = ref<Msg[]>([])
const history = ref<ChatMessage[]>([]) // OpenAI 兼容历史（不含 system）
const input = ref('')
const busy = ref(false)
const error = ref('')
const listRef = ref<HTMLElement>()
const inputRef = ref<HTMLTextAreaElement>()
const minimized = ref(false)
const barTop = ref(Math.round(window.innerHeight / 3))
const panelW = ref(380)
const panelH = ref(Math.round(window.innerHeight * 0.72))
let ctrl: AbortController | null = null

async function scrollBottom() {
  await nextTick()
  listRef.value?.scrollTo({ top: listRef.value.scrollHeight })
}

/** 最小化：只收起 UI，运行中的任务不动 */
function minimize() {
  barTop.value = Math.round(window.innerHeight / 3)
  minimized.value = true
}

/** 关闭即取消：中止进行中的请求与命令序列 */
function close() {
  ctrl?.abort()
  ctrl = null
  busy.value = false
  minimized.value = false
  show.value = false
}

/** 面板拉大：向左加宽 / 向下加高（左右下角手柄两者同时） */
function startResize(e: PointerEvent, dir: 'left' | 'bottom' | 'corner') {
  e.preventDefault()
  const el = e.currentTarget as HTMLElement
  // 锁定指针：即使拖进内核 iframe（其内部事件不冒泡到父页）也能收到 move/up；合成事件无活动指针时回退 window
  let target: HTMLElement | Window = window
  try {
    el.setPointerCapture(e.pointerId)
    target = el
  } catch { /* keep window */ }
  const sx = e.clientX
  const sy = e.clientY
  const w0 = panelW.value
  const h0 = panelH.value
  const move = (ev: Event) => {
    const p = ev as PointerEvent
    if (dir !== 'bottom') {
      panelW.value = Math.min(window.innerWidth - 60, Math.max(320, w0 + sx - p.clientX))
    }
    if (dir !== 'left') {
      panelH.value = Math.min(window.innerHeight, Math.max(280, h0 + p.clientY - sy))
    }
  }
  const up = () => {
    target.removeEventListener('pointermove', move)
    target.removeEventListener('pointerup', up)
    target.removeEventListener('pointercancel', up)
    window.removeEventListener('blur', up)
  }
  target.addEventListener('pointermove', move)
  target.addEventListener('pointerup', up)
  target.addEventListener('pointercancel', up)
  window.addEventListener('blur', up)
}

/** 最小化小条：按住上下拖拽，点击（未拖动时）展开 */
let dragStartY = 0
let dragged = false
function onBarDown(e: PointerEvent) {
  const el = e.currentTarget as HTMLElement
  // 锁定指针：拖进内核 iframe 松开也能收到 up，避免监听残留导致"不按住也拖"
  let target: HTMLElement | Window = window
  try {
    el.setPointerCapture(e.pointerId)
    target = el
  } catch { /* keep window */ }
  dragStartY = e.clientY - barTop.value
  dragged = false
  const move = (ev: Event) => {
    const p = ev as PointerEvent
    const top = p.clientY - dragStartY
    if (Math.abs(top - barTop.value) > 3) dragged = true
    barTop.value = Math.min(window.innerHeight - 110, Math.max(0, top))
  }
  const up = () => {
    target.removeEventListener('pointermove', move)
    target.removeEventListener('pointerup', up)
    target.removeEventListener('pointercancel', up)
    window.removeEventListener('blur', up)
  }
  target.addEventListener('pointermove', move)
  target.addEventListener('pointerup', up)
  target.addEventListener('pointercancel', up)
  window.addEventListener('blur', up)
}
function onBarClick() {
  if (!dragged) {
    minimized.value = false
    show.value = true
  }
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
  const live = messages.value.push({ role: 'assistant', text: '' }) - 1
  void scrollBottom()

  ctrl = new AbortController()
  converse({
    kernel: props.kernel,
    bridge: props.bridge,
    history: history.value,
    text,
    signal: ctrl.signal,
    onDelta: (liveText) => {
      messages.value[live].text = liveText
      void scrollBottom()
    },
  })
    .then((reply) => {
      // 命令执行完补最终展示文本；纯命令回复（剥标签后无文字）不留白泡
      const shown = liveDisplay(reply)
      if (shown) messages.value[live].text = shown
      else messages.value.splice(live, 1)
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
