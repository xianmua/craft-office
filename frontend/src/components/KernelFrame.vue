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
  if (event.origin !== location.origin) return
  // 只听本 iframe 发来的消息
  if (event.source && frame.value?.contentWindow && event.source !== frame.value.contentWindow) return
  const parsed = parseKernelMessage(props.kernel, event.data)
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
  frame.value.contentWindow.postMessage(openMessage(name, bytes), location.origin)
}

onMounted(() => window.addEventListener('message', onMessage))
onBeforeUnmount(() => window.removeEventListener('message', onMessage))

defineExpose({ openDoc })
</script>
