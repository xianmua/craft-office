<template>
  <!-- 通用吸顶侧面板容器：高度默认占满视口，宽度可由左缘拖拽调整；
       头部（标题/最小化/关闭）+ 内容 slot + 底部 footer slot，助手/设置等共用 -->
  <div
    v-if="visible"
    class="fixed top-0 right-4 z-30 flex flex-col bg-white shadow-2xl overflow-hidden"
    :style="{ width: w + 'px', height: '100vh' }"
  >
    <div class="absolute left-0 top-0 bottom-0 w-2 cursor-ew-resize z-10" @pointerdown="startResize"></div>

    <div class="flex items-center gap-2 pl-4 pr-2 h-11 shrink-0 border-b border-gray-100">
      <span class="text-sm font-medium">{{ title }}</span>
      <span v-if="busy" class="text-xs text-gray-400">任务运行中…</span>
      <div class="flex-1"></div>
      <slot name="actions" />
      <button
        class="w-6 h-6 flex items-center justify-center border-0 bg-transparent p-0 text-gray-400 hover:text-gray-600 cursor-pointer"
        title="最小化"
        @click="emit('minimize')"
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
          <path d="M6 3l5 5-5 5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
      <button
        class="w-6 h-6 flex items-center justify-center border-0 bg-transparent p-0 text-gray-400 hover:text-gray-600 cursor-pointer"
        :title="closeTitle"
        @click="emit('close')"
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
        </svg>
      </button>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto">
      <slot />
    </div>
    <slot name="footer" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    visible: boolean
    title: string
    busy?: boolean
    closeTitle?: string
    width?: number
  }>(),
  { busy: false, closeTitle: '关闭', width: 380 },
)

const emit = defineEmits<{ (e: 'minimize'): void; (e: 'close'): void }>()

const w = ref(props.width)

/** 左缘拖拽调宽（高度占满后只有宽度可调）；指针锁定防 iframe 吞事件 */
function startResize(e: PointerEvent) {
  e.preventDefault()
  const el = e.currentTarget as HTMLElement
  let target: HTMLElement | Window = window
  try {
    el.setPointerCapture(e.pointerId)
    target = el
  } catch { /* keep window */ }
  const sx = e.clientX
  const w0 = w.value
  const move = (ev: Event) => {
    const p = ev as PointerEvent
    w.value = Math.min(window.innerWidth - 60, Math.max(320, w0 + sx - p.clientX))
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
</script>
