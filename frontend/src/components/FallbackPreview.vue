<template>
  <div class="h-full overflow-auto px-6 py-5 bg-gray-100">
    <div class="max-w-5xl mx-auto flex flex-col gap-3">
      <t-alert theme="warning" :close="false">
        <template #message>
          未检测到 ArtCraft 内核，当前为降级只读预览（结构解析在浏览器本地完成，文件内容不会上传）。
          接入内核后即可编辑与导出，见 vendor/artcraft/README.md。
        </template>
      </t-alert>

      <t-alert v-for="(note, i) in model.notes" :key="i" theme="info" :close="false">
        <template #message>{{ note }}</template>
      </t-alert>

      <!-- docx -->
      <article
        v-if="model.kind === 'docx'"
        class="bg-white shadow-sm border border-gray-200 rounded-md px-10 py-8 min-h-[60vh]"
      >
        <p
          v-for="(p, i) in model.paragraphs"
          :key="i"
          class="my-2 leading-7 text-[15px] text-gray-800 whitespace-pre-wrap"
        >
          {{ p }}
        </p>
        <div v-if="!model.paragraphs?.length" class="text-muted text-sm">该文档没有可提取的正文段落。</div>
      </article>

      <!-- xlsx -->
      <div v-else-if="model.kind === 'xlsx'" class="flex flex-col gap-4">
        <section v-for="sheet in sheetViews" :key="sheet.name" class="bg-white rounded-md border border-gray-200 overflow-hidden">
          <div class="px-4 py-2 bg-gray-50 border-b border-gray-200 flex items-center gap-2">
            <span class="font-medium text-sm">{{ sheet.name }}</span>
            <t-tag v-if="sheet.truncated" size="small" theme="default">已裁剪</t-tag>
          </div>
          <div class="overflow-auto max-h-[60vh]">
            <table class="border-collapse text-[13px]">
              <thead>
                <tr>
                  <th class="sticky left-0 bg-gray-50 border border-gray-200 w-10 text-muted font-normal" />
                  <th
                    v-for="c in sheet.width"
                    :key="c"
                    class="bg-gray-50 border border-gray-200 px-3 py-1 text-muted font-normal min-w-[80px]"
                  >
                    {{ columnName(c - 1) }}
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(row, r) in sheet.rows" :key="r">
                  <td class="sticky left-0 bg-gray-50 border border-gray-200 text-center text-muted">{{ r + 1 }}</td>
                  <td
                    v-for="(cell, c) in row"
                    :key="c"
                    class="border border-gray-200 px-3 py-1 align-top whitespace-nowrap max-w-[280px] overflow-hidden text-ellipsis"
                  >
                    {{ cell }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>
      </div>

      <!-- pptx -->
      <div v-else class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
        <section
          v-for="slide in model.slides"
          :key="slide.index"
          class="bg-white rounded-md border border-gray-200 p-5 min-h-[180px] flex flex-col gap-2"
        >
          <div class="text-muted text-xs">第 {{ slide.index }} 页</div>
          <div v-for="(line, i) in slide.texts" :key="i" class="text-sm text-gray-800 leading-6">{{ line }}</div>
          <div v-if="!slide.texts.length" class="text-muted text-xs">（该页没有文本框内容）</div>
        </section>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import type { PreviewModel } from '@/artcraft/ooxml'

const props = defineProps<{ model: PreviewModel }>()

const sheetViews = computed(() =>
  (props.model.sheets ?? []).map((sheet) => {
    let width = 0
    sheet.rows.forEach((row) => {
      row.forEach((cell, i) => {
        if (cell) width = Math.max(width, i + 1)
      })
    })
    const rows = sheet.rows.map((row) => row.slice(0, width || 1))
    while (rows.length > 0) {
      const last = rows[rows.length - 1]!
      if (last.some((c) => c)) break
      rows.pop()
    }
    return { name: sheet.name, rows, width: width || 1, truncated: sheet.truncated }
  }),
)

function columnName(index: number): string {
  let n = index + 1
  let name = ''
  while (n > 0) {
    const rem = (n - 1) % 26
    name = String.fromCharCode(65 + rem) + name
    n = Math.floor((n - 1) / 26)
  }
  return name
}
</script>
