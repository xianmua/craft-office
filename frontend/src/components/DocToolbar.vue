<template>
  <footer
    class="h-12 shrink-0 px-4 flex items-center gap-3 bg-white border-t border-gray-200 text-sm text-gray-600"
  >
    <span class="flex items-center gap-1">
      <span class="w-2 h-2 rounded-full" :class="mode === 'editor' ? 'bg-green-500' : 'bg-amber-500'" />
      {{ mode === 'editor' ? 'ArtCraft 内核已接入' : '降级只读预览' }}
    </span>

    <span v-if="sizeText" class="text-muted">· {{ sizeText }}</span>
    <span v-if="entryCount" class="text-muted">· {{ entryCount }} 个 OOXML 部件</span>

    <div class="flex-1" />

    <span class="text-muted hidden sm:inline">只读模式</span>
    <t-switch :value="readOnly" size="small" :disabled="mode !== 'editor'" @change="onToggleReadOnly" />

    <t-button size="small" variant="outline" @click="emit('copy')">复制链接</t-button>
    <t-button size="small" theme="primary" :disabled="mode !== 'editor'" @click="emit('export')">
      导出为文件
    </t-button>
    <a href="/wordcraft/" target="_blank" rel="noopener" class="no-underline">
      <t-button size="small" variant="dashed">完整编辑器 ↗</t-button>
    </a>
    <t-button v-if="mode !== 'editor'" size="small" @click="emit('download-original')">下载原文件</t-button>
  </footer>
</template>

<script setup lang="ts">
const props = withDefaults(
  defineProps<{
    mode: 'editor' | 'preview' | ''
    readOnly: boolean
    sizeText?: string
    entryCount?: number
  }>(),
  { sizeText: '', entryCount: 0 },
)

const emit = defineEmits<{
  (e: 'toggle-readonly', value: boolean): void
  (e: 'export'): void
  (e: 'copy'): void
  (e: 'download-original'): void
}>()

function onToggleReadOnly(value: boolean | string | number) {
  const next = Boolean(value)
  emit('toggle-readonly', next)
}
</script>
