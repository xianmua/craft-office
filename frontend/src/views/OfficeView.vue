<template>
  <div class="page-shell bg-gray-50">
    <header class="h-14 shrink-0 flex items-center gap-3 px-4 bg-white border-b border-gray-200">
      <span class="font-semibold text-[15px]">ArtCraft Office</span>
      <t-tag v-if="kindLabel" size="small" variant="light">{{ kindLabel }}</t-tag>
      <span class="text-sm text-muted truncate max-w-[45%]">{{ fileName || '未加载文档' }}</span>
      <div class="flex-1" />
      <t-button size="small" variant="text" @click="openLinkDialog">更换链接</t-button>
      <t-button size="small" variant="outline" :disabled="!remoteUrl" @click="load">重新加载</t-button>
    </header>

    <main class="relative flex-1 min-h-0">
      <div v-if="loading" class="absolute inset-0 z-10 flex-center flex-col gap-3 bg-white/85">
        <t-loading size="small" />
        <span class="text-sm text-muted">正在通过后端代理拉取文档…</span>
      </div>

      <!-- 未带 url 参数：入口 -->
      <div v-if="!hasUrl" class="h-full flex-center px-6">
        <div class="w-full max-w-xl surface-card p-8 flex flex-col gap-4">
          <h1 class="text-xl font-semibold m-0">单人在线 Office</h1>
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

      <!-- 正文 -->
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
        />
        <FallbackPreview v-else-if="preview" :model="preview" />
      </template>
    </main>

    <DocToolbar
      v-if="ready"
      :mode="mode"
      :dirty="dirty"
      :size-text="sizeText"
      :entry-count="preview?.entryCount ?? 0"
      @copy="copyLink"
      @download-original="downloadOriginal"
    />

    <t-dialog
      v-model:visible="linkDialog"
      header="更换文档链接"
      :confirm-btn="{ content: '打开' }"
      @confirm="applyLink"
    >
      <t-input v-model="linkInput" placeholder="https://cdn.example.com/demo.docx" />
    </t-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, nextTick, ref } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import { ErrorCircleFilledIcon } from 'tdesign-icons-vue-next'
import KernelFrame from '@/components/KernelFrame.vue'
import FallbackPreview from '@/components/FallbackPreview.vue'
import DocToolbar from '@/components/DocToolbar.vue'
import { useDocSource, fileNameOf } from '@/composables/useDocSource'
import { downloadUrl, fetchBinary, fetchHealth, fetchMeta } from '@/api/doc'
import { KERNEL_OF, type KernelSavePayload } from '@/artcraft/loader'
import { inspectOoxml, type PreviewModel } from '@/artcraft/ooxml'

const { remoteUrl, kind, errorText, parse } = useDocSource()

const loading = ref(false)
const ready = ref(false)
const mode = ref<'editor' | 'preview' | ''>('')
const preview = ref<PreviewModel | null>(null)
const binary = ref<ArrayBuffer | null>(null)
const sizeText = ref('')
const dirty = ref(false)
const editorKernel = ref('')
const kernelRef = ref<InstanceType<typeof KernelFrame>>()

const hasUrl = ref(false)
const linkDialog = ref(false)
const linkInput = ref('')

const fileName = computed(() => (remoteUrl.value ? fileNameOf(remoteUrl.value) : ''))
const kindLabel = computed(() => (kind.value === 'unknown' ? '' : kind.value.toUpperCase()))
const binaryData = computed(() => binary.value ?? new ArrayBuffer(0))

function formatSize(size: number | null): string {
  if (size === null || size === undefined) return '大小未知'
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / 1024 / 1024).toFixed(2)} MB`
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
  sizeText.value = ''
  dirty.value = false
  editorKernel.value = ''

  try {
    const meta = await fetchMeta(remoteUrl.value)
    if (!meta.supported) throw new Error(`后端判定该格式不受支持：.${meta.ext}`)
    sizeText.value = formatSize(meta.size)

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
