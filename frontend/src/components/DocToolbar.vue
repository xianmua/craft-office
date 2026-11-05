<template>
  <footer
    class="h-12 shrink-0 px-4 flex items-center gap-3 bg-white border-t border-gray-200 text-sm text-gray-600"
  >
    <span class="flex items-center gap-1">
      <span class="w-2 h-2 rounded-full" :class="mode === 'editor' ? 'bg-green-500' : 'bg-amber-500'" />
      {{ mode === 'editor' ? 'WordCraft 内核编辑中' : '降级只读预览' }}
    </span>
    <span v-if="dirty" class="text-amber-600">· 有未保存修改（编辑器内 Ctrl+S 保存并下载）</span>

    <span v-if="sizeText" class="text-muted">· {{ sizeText }}</span>
    <span v-if="entryCount" class="text-muted">· {{ entryCount }} 个 OOXML 部件</span>

    <div class="flex-1" />

    <t-button size="small" variant="outline" @click="emit('copy')">复制链接</t-button>
    <t-button size="small" variant="outline" @click="emit('download-original')">下载原文件</t-button>
    <a href="/wordcraft/" target="_blank" rel="noopener" class="no-underline">
      <t-button size="small" variant="dashed">完整编辑器 ↗</t-button>
    </a>
  </footer>
</template>

<script setup lang="ts">
withDefaults(
  defineProps<{
    mode: 'editor' | 'preview' | ''
    dirty?: boolean
    sizeText?: string
    entryCount?: number
  }>(),
  { dirty: false, sizeText: '', entryCount: 0 },
)

const emit = defineEmits<{
  (e: 'copy'): void
  (e: 'download-original'): void
}>()
</script>
