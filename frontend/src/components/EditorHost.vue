<template>
  <div ref="host" class="w-full h-full overflow-auto bg-white" />
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { createEditor, type DocKind, type EditorInstance } from '@/artcraft/loader'

const props = defineProps<{
  kind: DocKind
  binary: ArrayBuffer
  fileName: string
  readOnly: boolean
}>()

const emit = defineEmits<{
  (e: 'ready'): void
  (e: 'error', message: string): void
}>()

const host = ref<HTMLElement>()
let inst: EditorInstance | null = null

onMounted(async () => {
  if (!host.value) return
  try {
    inst = await createEditor(props.kind, {
      container: host.value,
      binary: props.binary,
      fileName: props.fileName,
      readOnly: props.readOnly,
    })
    if (!inst) {
      emit('error', '内核未就绪')
      return
    }
    emit('ready')
  } catch (err) {
    emit('error', err instanceof Error ? err.message : '内核初始化失败')
  }
})

onBeforeUnmount(() => {
  inst?.destroy()
  inst = null
})

defineExpose({
  exportFile: (): Promise<Blob> | null => (inst ? inst.save() : null),
  setReadOnly: (value: boolean) => inst?.setReadOnly(value),
})
</script>
