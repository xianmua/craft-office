<template>
  <iframe
    ref="frame"
    class="w-full h-full block border-0 bg-white"
    :src="frameSrc"
    :title="`${kernel} 编辑器`"
  />
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { kernelFrameUrl, openMessage, parseKernelMessage, type KernelSavePayload } from '@/artcraft/loader'

const props = defineProps<{
  kernel: string
}>()

const emit = defineEmits<{
  (e: 'ready'): void
  (e: 'failed', message: string): void
  (e: 'save', payload: KernelSavePayload): void
  (e: 'dirty', value: boolean): void
}>()

const frame = ref<HTMLIFrameElement>()
const frameSrc = kernelFrameUrl(props.kernel)

let kernelReady = false
const pending: Array<{ name: string; bytes: ArrayBuffer }> = []

function onMessage(event: MessageEvent) {
  console.log('[kf] message', event.origin, typeof event.data, JSON.stringify(event.data))
  if (event.origin !== location.origin) return
  // 不做 event.source 与 iframe.contentWindow 的严格比对：内核加载期间 iframe 可能被
  // Vue 重建，旧内核窗口的迟发消息会被误丢；同源 + 消息类型前缀已足够过滤。
  const parsed = parseKernelMessage(props.kernel, event.data)
  console.log('[kf] parsed kernel=', JSON.stringify(props.kernel), '->', JSON.stringify(parsed))
  if (!parsed) return
  switch (parsed.type) {
    case 'ready':
      kernelReady = true
      emit('ready')
      flush()
      break
    case 'failed':
      emit('failed', parsed.message)
      break
    case 'save':
      emit('save', parsed.payload)
      break
    case 'dirty':
      emit('dirty', parsed.value)
      break
  }
}

function flush() {
  while (kernelReady && pending.length && frame.value?.contentWindow) {
    const doc = pending.shift()
    if (doc) openDoc(doc.name, doc.bytes)
  }
}

/** 在内核中打开文档；未就绪时排队，ready 后自动发出 */
function openDoc(name: string, bytes: ArrayBuffer) {
  if (!kernelReady || !frame.value?.contentWindow) {
    if (pending.length < 4) pending.push({ name, bytes })
    return
  }
  frame.value.contentWindow.postMessage(openMessage(props.kernel, name, bytes), location.origin)
}

/**
 * 官方页面命令 API 薄代理：调用内核窗口上的 window.<app>.<method>（同源 iframe 可直接访问）。
 * method ∈ request/execute/commands/docInspect/uiInspect/screenshot，均返回 promise（内核自带超时）。
 * 仅新快照内核（gridcraft 起）注入了 window.<app>；旧内核无此对象时返回 null。
 */
function kernelApi(): Record<string, (...args: unknown[]) => Promise<unknown>> | null {
  const win = frame.value?.contentWindow as (Window & Record<string, unknown>) | null | undefined
  const api = win?.[props.kernel]
  return api && typeof api === 'object' ? (api as Record<string, (...args: unknown[]) => Promise<unknown>>) : null
}

function call(method: string, ...args: unknown[]): Promise<unknown> {
  const api = kernelApi()
  const fn = api?.[method]
  if (typeof fn !== 'function') return Promise.reject(new Error(`内核 ${props.kernel} 不支持 ${method}（缺少页面命令 API）`))
  return fn.apply(api, args)
}

onMounted(() => window.addEventListener('message', onMessage))
onBeforeUnmount(() => window.removeEventListener('message', onMessage))

defineExpose({ openDoc, call, kernelApi })
</script>
