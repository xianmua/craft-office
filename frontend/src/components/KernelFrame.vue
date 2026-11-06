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
  (e: 'plugins'): void
}>()

const frame = ref<HTMLIFrameElement>()
const frameSrc = kernelFrameUrl(props.kernel)

let kernelReady = false
const pending: Array<{ name: string; bytes: ArrayBuffer }> = []

function onMessage(event: MessageEvent) {
  console.log('[kf] message', event.origin, typeof event.data, JSON.stringify(event.data))
  if (event.origin !== location.origin) return
  // 不做 event.source 与 iframe.contentWindow 的严格比对：内核加载期间 iframe 可能被
  // Vue 重建，旧内核窗口的迟发消息（如 wordcraft:plugins）会被误丢，导致市场打不开；
  // 同源 + 消息类型前缀（parseKernelMessage 按 `${kernel}:` 匹配）已足够过滤。
  const parsed = parseKernelMessage(props.kernel, event.data)
  console.log('[kf] parsed kernel=', JSON.stringify(props.kernel), '->', JSON.stringify(parsed))
  if (!parsed) return
  switch (parsed.type) {
    case 'ready':
      kernelReady = true
      emit('ready')
      flush()
      break
    case 'plugins':
      emit('plugins')
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
  frame.value.contentWindow.postMessage(openMessage(name, bytes), location.origin)
}

/** 插件启停/安装/卸载后通知内核重载插件（web/plugins.rs 监听） */
function notifyPluginsChanged() {
  frame.value?.contentWindow?.postMessage('wordcraft:pluginsChanged', location.origin)
}

onMounted(() => window.addEventListener('message', onMessage))
onBeforeUnmount(() => window.removeEventListener('message', onMessage))

defineExpose({ openDoc, notifyPluginsChanged })
</script>
