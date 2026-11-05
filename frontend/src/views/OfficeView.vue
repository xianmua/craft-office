<template>
  <div class="page-shell bg-gray-50">
    <main class="relative flex-1 min-h-0">
      <div v-if="loading" class="absolute inset-0 z-10 flex-center flex-col gap-3 bg-white/85">
        <t-loading size="small" />
        <span class="text-sm text-muted">正在通过后端代理拉取文档…</span>
      </div>

      <!-- 未带 url 参数：入口 -->
      <div v-if="!hasUrl" class="h-full flex-center px-6">
        <div class="w-full max-w-xl surface-card p-8 flex flex-col gap-4">
          <h1 class="text-xl font-semibold m-0">ArtCraft Office</h1>
          <p class="text-sm text-muted m-0 leading-6">
            在地址栏拼接 <code class="px-1 bg-gray-100 rounded">?url=</code> 打开远程 Office 文件，也可以直接粘贴直链：
          </p>
          <t-input
            v-model="linkInput"
            placeholder="粘贴 docx / xlsx / pptx 直链，例如 https://cdn.example.com/demo.docx"
            @enter="applyLink"
          />
          <div class="flex gap-2">
            <t-button theme="primary" :disabled="!linkInput" @click="applyLink">打开文档</t-button>
            <t-button variant="outline" @click="fillDemo">填入示例链接</t-button>
          </div>
          <p class="text-xs text-muted m-0">
            仅允许代理后端白名单内的域名；内网地址、非 443 端口会被拒绝。
          </p>
        </div>
      </div>

      <!-- 错误 -->
      <div v-else-if="errorText" class="h-full flex-center flex-col gap-3 px-6 text-center">
        <ErrorCircleFilledIcon size="2.2rem" class="text-orange-500" />
        <p class="text-sm text-gray-600 m-0 max-w-xl leading-6">{{ errorText }}</p>
        <div class="flex gap-2 mt-1">
          <t-button size="small" variant="outline" @click="openLinkDialog">更换链接</t-button>
          <t-button size="small" @click="load">重试</t-button>
        </div>
      </div>

      <!-- 正文：内核 / 预览全屏，功能收进悬浮球 -->
      <template v-else-if="ready">
        <KernelFrame
          v-if="mode === 'editor' && editorKernel"
          ref="kernelRef"
          class="h-full"
          :kernel="editorKernel"
          @ready="onKernelReady"
          @save="onKernelSave"
          @dirty="onKernelDirty"
          @failed="onKernelFailed"
          @plugins="openMarket"
        />
        <FallbackPreview v-else-if="preview" :model="preview" />

        <!-- 悬浮球 -->
        <div class="absolute top-3 right-3 z-30">
          <button
            class="relative w-11 h-11 rounded-full bg-white border border-gray-200 shadow-lg flex-center hover:bg-gray-50 transition-colors"
            title="ArtCraft Office 菜单"
            @click="menuOpen = !menuOpen"
          >
            <span class="text-[13px] font-semibold text-gray-700">AC</span>
            <span v-if="dirty" class="absolute -top-0.5 -right-0.5 w-3 h-3 rounded-full bg-amber-400 border-2 border-white" />
          </button>

          <div
            v-if="menuOpen"
            class="absolute right-0 mt-2 w-52 bg-white border border-gray-200 rounded-lg shadow-xl py-1.5 flex flex-col"
          >
            <span class="px-4 pt-1 pb-1.5 text-xs text-muted truncate">{{ fileName }}</span>
            <span v-if="dirty" class="px-4 pb-1.5 text-xs text-amber-600">有未保存修改（编辑器内 Ctrl+S）</span>
            <div class="h-px bg-gray-100 my-1" />
            <t-button variant="text" class="menu-item" size="small" @click="openAi">AI 助手</t-button>
            <t-button variant="text" class="menu-item" size="small" @click="openMarket">插件市场</t-button>
            <template v-if="pluginCommands.length">
              <div class="h-px bg-gray-100 my-1" />
              <t-button
                v-for="cmd in pluginCommands"
                :key="cmd.id"
                variant="text"
                class="menu-item"
                size="small"
                @click="runCommand(cmd)"
              >
                {{ cmd.title }}
              </t-button>
            </template>
            <div class="h-px bg-gray-100 my-1" />
            <t-button variant="text" class="menu-item" size="small" @click="openLinkDialog">更换链接</t-button>
            <t-button variant="text" class="menu-item" size="small" :disabled="!remoteUrl" @click="load">重新加载</t-button>
            <t-button variant="text" class="menu-item" size="small" @click="copyLink">复制链接</t-button>
            <t-button variant="text" class="menu-item" size="small" @click="downloadOriginal">下载原文件</t-button>
            <a href="/wordcraft/" target="_blank" rel="noopener" class="no-underline">
              <t-button variant="text" class="menu-item" size="small">完整编辑器 ↗</t-button>
            </a>
          </div>
        </div>
        <div v-if="menuOpen" class="fixed inset-0 z-20" @click="menuOpen = false" />

        <AiPanel v-model:visible="aiOpen" :doc-context="docContext" />

        <!-- 插件输出对话框 -->
        <t-dialog
          v-model:visible="pluginDialog.visible"
          :header="pluginDialog.title"
          :footer="false"
          destroy-on-close
        >
          <p class="text-sm leading-6 whitespace-pre-wrap break-words m-0">{{ pluginDialog.content }}</p>
        </t-dialog>

        <t-dialog
          v-model:visible="linkDialog"
          header="更换文档链接"
          :confirm-btn="{ content: '打开' }"
          @confirm="applyLink"
        >
          <t-input v-model="linkInput" placeholder="https://cdn.example.com/demo.docx" @enter="applyLink" />
        </t-dialog>
      </template>

      <PluginMarket
        ref="marketRef"
        v-model:visible="marketOpen"
        @changed="reloadPlugins"
      />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, nextTick, ref } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import { ErrorCircleFilledIcon } from 'tdesign-icons-vue-next'
import KernelFrame from '@/components/KernelFrame.vue'
import FallbackPreview from '@/components/FallbackPreview.vue'
import AiPanel from '@/components/AiPanel.vue'
import PluginMarket from '@/components/PluginMarket.vue'
import { useDocSource, fileNameOf } from '@/composables/useDocSource'
import { downloadUrl, fetchBinary, fetchHealth, fetchMeta } from '@/api/doc'
import { KERNEL_OF, type KernelSavePayload } from '@/artcraft/loader'
import { inspectOoxml, type PreviewModel } from '@/artcraft/ooxml'
import { runPlugins, type PluginCommand } from '@/plugins/runtime'

const { remoteUrl, kind, errorText, parse } = useDocSource()

const loading = ref(false)
const ready = ref(false)
const mode = ref<'editor' | 'preview' | ''>('')
const preview = ref<PreviewModel | null>(null)
const binary = ref<ArrayBuffer | null>(null)
const dirty = ref(false)
const editorKernel = ref('')
const kernelRef = ref<InstanceType<typeof KernelFrame>>()

const menuOpen = ref(false)
const aiOpen = ref(false)
const marketOpen = ref(false)
const marketRef = ref<InstanceType<typeof PluginMarket>>()
const pluginCommands = ref<PluginCommand[]>([])
const pluginDialog = ref({ visible: false, title: '', content: '' })

const hasUrl = ref(false)
const linkInput = ref('')
const linkDialog = ref(false)

const fileName = computed(() => (remoteUrl.value ? fileNameOf(remoteUrl.value) : ''))

/** 文档纯文本（AI / 插件上下文）：从已拉取的字节本地解析，不上传文件本身 */
const docContext = computed(() => {
  if (!binary.value) return ''
  try {
    const docKind = kind.value === 'unknown' ? 'docx' : kind.value
    const model = inspectOoxml(docKind, new Uint8Array(binary.value))
    const parts: string[] = [`文件：${fileName.value || 'document'}`]
    if (model.paragraphs?.length) parts.push(model.paragraphs.join('\n'))
    for (const s of model.sheets ?? []) {
      parts.push(`[工作表 ${s.name}]`)
      parts.push(s.rows.map((r) => r.join('\t')).join('\n'))
    }
    for (const s of model.slides ?? []) parts.push(`[第 ${s.index} 页]\n${s.texts.join('\n')}`)
    const text = parts.join('\n')
    return text.length > 15000 ? `${text.slice(0, 15000)}\n（内容过长已截断）` : text
  } catch {
    return ''
  }
})

/** 插件宿主 API（执行插件时注入） */
function pluginHostApi() {
  return {
    docText: () => docContext.value.replace(/^文件：.*\n/, ''),
    aiChat: async (messages: Array<{ role: string; content: string }>) => {
      const res = await fetch('/api/ai/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ messages, stream: false }),
      })
      if (!res.ok) throw new Error(`AI 请求失败（HTTP ${res.status}）`)
      const body = await res.json()
      return body?.choices?.[0]?.message?.content ?? ''
    },
    toast: (message: string) => MessagePlugin.success(message),
    dialog: (title: string, content: string) => {
      pluginDialog.value = { visible: true, title, content }
    },
  }
}

async function reloadPlugins() {
  try {
    const loaded = await runPlugins(pluginHostApi())
    pluginCommands.value = loaded.flatMap((p) => p.commands)
    const failed = loaded.filter((p) => p.error)
    if (failed.length) MessagePlugin.warning(`有 ${failed.length} 个插件加载失败`)
  } catch {
    /* 插件目录不可用时静默：核心编辑不受影响 */
  }
}

async function runCommand(cmd: PluginCommand) {
  menuOpen.value = false
  try {
    await cmd.run()
  } catch (err) {
    MessagePlugin.error(err instanceof Error ? err.message : '插件执行失败')
  }
}

function openAi() {
  menuOpen.value = false
  aiOpen.value = true
}

function openMarket() {
  menuOpen.value = false
  marketOpen.value = true
  void marketRef.value?.refresh()
}

async function load() {
  if (!remoteUrl.value) {
    hasUrl.value = false
    return
  }

  hasUrl.value = true
  loading.value = true
  ready.value = false
  mode.value = ''
  preview.value = null
  binary.value = null
  dirty.value = false
  editorKernel.value = ''
  pluginCommands.value = []

  try {
    const meta = await fetchMeta(remoteUrl.value)
    if (!meta.supported) throw new Error(`后端判定该格式不受支持：.${meta.ext}`)

    const bytes = await fetchBinary(remoteUrl.value)
    binary.value = bytes

    const docKind = kind.value === 'unknown' ? undefined : kind.value
    const kernelName = docKind ? KERNEL_OF[docKind] : undefined
    const health = await fetchHealth().catch(() => ({ editor_kernel_ready: false, max_bytes: 0 }))

    if (health.editor_kernel_ready && kernelName) {
      // 内核模式：iframe 就绪后由 onKernelReady 投递文档字节
      editorKernel.value = kernelName
      mode.value = 'editor'
      await nextTick()
    } else {
      preview.value = inspectOoxml(docKind ?? 'docx', new Uint8Array(bytes))
      mode.value = 'preview'
    }

    ready.value = true
    void reloadPlugins()
  } catch (err) {
    errorText.value = err instanceof Error ? err.message : '文档加载失败'
    ready.value = false
  } finally {
    loading.value = false
  }
}

function onKernelReady() {
  if (!binary.value) return
  kernelRef.value?.openDoc(fileName.value || 'document.docx', binary.value)
}

function onKernelSave(payload: KernelSavePayload) {
  const blob = new Blob([payload.bytes])
  const a = document.createElement('a')
  a.href = URL.createObjectURL(blob)
  a.download = payload.name
  a.click()
  URL.revokeObjectURL(a.href)
  dirty.value = false
  MessagePlugin.success(`已保存并下载：${payload.name}`)
}

function onKernelDirty(value: boolean) {
  dirty.value = value
}

function onKernelFailed(message: string) {
  MessagePlugin.error(`内核启动失败：${message}`)
  if (binary.value) {
    try {
      preview.value = inspectOoxml(kind.value === 'unknown' ? 'docx' : kind.value, new Uint8Array(binary.value))
      mode.value = 'preview'
    } catch {
      errorText.value = message
      ready.value = false
    }
  }
}

function openLinkDialog() {
  menuOpen.value = false
  linkInput.value = remoteUrl.value
  linkDialog.value = true
}

function applyLink() {
  const raw = linkInput.value.trim()
  if (!raw) {
    MessagePlugin.warning('请先粘贴文档直链')
    return
  }
  const next = `?url=${encodeURIComponent(raw)}`
  history.replaceState(null, '', `${location.pathname}${next}`)
  linkDialog.value = false
  parse()
  void load()
}

function fillDemo() {
  linkInput.value = 'https://calibre-ebook.com/downloads/demos/demo.docx'
}

function downloadOriginal() {
  if (!remoteUrl.value) return
  location.href = downloadUrl(remoteUrl.value)
}

async function copyLink() {
  try {
    await navigator.clipboard.writeText(location.href)
    MessagePlugin.success('链接已复制')
  } catch {
    MessagePlugin.warning('复制失败，请手动复制地址栏')
  }
}

onMounted(() => {
  parse()
  hasUrl.value = new URLSearchParams(location.search).has('url')
  if (hasUrl.value && !errorText.value) void load()
})
</script>

<style scoped>
.menu-item {
  justify-content: flex-start;
  width: 100%;
  text-align: left;
  --td-radius-default: 0;
}
</style>
