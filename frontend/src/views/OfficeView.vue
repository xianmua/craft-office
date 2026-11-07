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
            在地址栏拼接 <code class="px-1 bg-gray-100 rounded">?url=</code> 打开远程 Office
            文件，也可以直接粘贴直链：
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
          <!-- 产品坞：市场全集画成一排插座，已开通的亮，未开通的置灰，点击提示 -->
          <div v-if="dock.length" class="flex flex-wrap gap-3 mt-1">
            <div
              v-for="item in dock"
              :key="item.id"
              class="dock-socket"
              :class="{ enabled: item.enabled }"
              @click="onSocketClick(item)"
            >
              <span class="socket-hole"><span class="socket-dot" /></span>
              <span class="socket-name">{{ item.name }}</span>
            </div>
          </div>
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

      <!-- 正文：内核 / 预览全屏 -->
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

        <t-dialog
          v-model:visible="linkDialog"
          header="更换文档链接"
          :confirm-btn="{ content: '打开' }"
          @confirm="applyLink"
        >
          <t-input
            v-model="linkInput"
            placeholder="https://cdn.example.com/demo.docx"
            @enter="applyLink"
          />
        </t-dialog>
      </template>

      <PluginMarket ref="marketRef" v-model:visible="marketOpen" @changed="onPluginsChanged" />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, nextTick, ref } from "vue";
import { MessagePlugin } from "tdesign-vue-next";
import { ErrorCircleFilledIcon } from "tdesign-icons-vue-next";
import KernelFrame from "@/components/KernelFrame.vue";
import FallbackPreview from "@/components/FallbackPreview.vue";
import PluginMarket from "@/components/PluginMarket.vue";
import { useDocSource, fileNameOf } from "@/composables/useDocSource";
import { fetchBinary, fetchHealth, fetchMeta } from "@/api/doc";
import { fetchPlugins } from "@/api/plugins";
import { KERNEL_OF, type KernelSavePayload } from "@/artcraft/loader";
import { inspectOoxml, type PreviewModel } from "@/artcraft/ooxml";

const { remoteUrl, kind, errorText, parse } = useDocSource();

const loading = ref(false);
const ready = ref(false);
const mode = ref<"editor" | "preview" | "">("");
const preview = ref<PreviewModel | null>(null);
const binary = ref<ArrayBuffer | null>(null);
const dirty = ref(false);
const editorKernel = ref("");
const kernelRef = ref<InstanceType<typeof KernelFrame>>();

const marketOpen = ref(false);
const marketRef = ref<InstanceType<typeof PluginMarket>>();

const hasUrl = ref(false);
const linkInput = ref("");
const linkDialog = ref(false);

/** 产品坞：市场全集 + 启停状态（失败时留空） */
interface DockItem {
  id: string;
  name: string;
  enabled: boolean;
}
const dock = ref<DockItem[]>([]);

async function refreshEnabledPlugins() {
  try {
    const catalog = await fetchPlugins();
    const enabled = new Set(catalog.installed.filter((p) => p.enabled).map((p) => p.id));
    dock.value = catalog.market.map((p) => ({
      id: p.id,
      name: p.name,
      enabled: enabled.has(p.id),
    }));
  } catch {
    dock.value = [];
  }
}

function onSocketClick(item: DockItem) {
  if (!item.enabled) MessagePlugin.warning(`「${item.name}」未开通`);
}

const fileName = computed(() => (remoteUrl.value ? fileNameOf(remoteUrl.value) : ""));

function openMarket() {
  console.log("[ov] openMarket");
  marketOpen.value = true;
  void marketRef.value?.refresh();
}

/** 市场里安装/卸载/启停后：让内核重载插件 */
function onPluginsChanged() {
  kernelRef.value?.notifyPluginsChanged();
}

async function load() {
  if (!remoteUrl.value) {
    hasUrl.value = false;
    return;
  }

  hasUrl.value = true;
  loading.value = true;
  ready.value = false;
  mode.value = "";
  preview.value = null;
  binary.value = null;
  dirty.value = false;
  editorKernel.value = "";

  try {
    const meta = await fetchMeta(remoteUrl.value);
    if (!meta.supported) throw new Error(`后端判定该格式不受支持：.${meta.ext}`);

    const bytes = await fetchBinary(remoteUrl.value);
    binary.value = bytes;

    const docKind = kind.value === "unknown" ? undefined : kind.value;
    const kernelName = docKind ? KERNEL_OF[docKind] : undefined;
    const health = await fetchHealth().catch(() => ({ editor_kernel_ready: false, max_bytes: 0 }));

    if (health.editor_kernel_ready && kernelName) {
      // 内核模式：iframe 就绪后由 onKernelReady 投递文档字节
      editorKernel.value = kernelName;
      mode.value = "editor";
      await nextTick();
    } else {
      preview.value = inspectOoxml(docKind ?? "docx", new Uint8Array(bytes));
      mode.value = "preview";
    }

    ready.value = true;
  } catch (err) {
    errorText.value = err instanceof Error ? err.message : "文档加载失败";
    ready.value = false;
  } finally {
    loading.value = false;
  }
}

function onKernelReady() {
  if (!binary.value) return;
  kernelRef.value?.openDoc(fileName.value || "document.docx", binary.value);
}

function onKernelSave(payload: KernelSavePayload) {
  const blob = new Blob([payload.bytes]);
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = payload.name;
  a.click();
  URL.revokeObjectURL(a.href);
  dirty.value = false;
  MessagePlugin.success(`已保存并下载：${payload.name}`);
}

function onKernelDirty(value: boolean) {
  dirty.value = value;
}

function onKernelFailed(message: string) {
  MessagePlugin.error(`内核启动失败：${message}`);
  if (binary.value) {
    try {
      preview.value = inspectOoxml(
        kind.value === "unknown" ? "docx" : kind.value,
        new Uint8Array(binary.value),
      );
      mode.value = "preview";
    } catch {
      errorText.value = message;
      ready.value = false;
    }
  }
}

function openLinkDialog() {
  linkInput.value = remoteUrl.value;
  linkDialog.value = true;
}

function applyLink() {
  const raw = linkInput.value.trim();
  if (!raw) {
    MessagePlugin.warning("请先粘贴文档直链");
    return;
  }
  const next = `?url=${encodeURIComponent(raw)}`;
  history.replaceState(null, "", `${location.pathname}${next}`);
  linkDialog.value = false;
  parse();
  void load();
}

function fillDemo() {
  linkInput.value = "/samples/demo.docx";
}

onMounted(() => {
  parse();
  hasUrl.value = new URLSearchParams(location.search).has("url");
  if (hasUrl.value && !errorText.value) void load();
  void refreshEnabledPlugins();
});
</script>

<style scoped>
.dock-socket {
  width: 84px;
  padding: 12px 0 10px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  border: 1px solid var(--td-component-stroke);
  border-radius: 8px;
  background: var(--td-bg-color-container);
  color: var(--td-text-color-placeholder);
  cursor: pointer;
}
.dock-socket.enabled {
  border-color: var(--td-brand-color);
  background: var(--td-brand-color-light);
  color: var(--td-text-color-primary);
}
.socket-hole {
  width: 26px;
  height: 26px;
  border-radius: 50%;
  border: 1.5px solid currentColor;
  display: flex;
  align-items: center;
  justify-content: center;
}
.socket-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: currentColor;
}
.dock-socket.enabled .socket-dot {
  background: var(--td-brand-color);
}
.socket-name {
  font-size: 12px;
  line-height: 1;
  white-space: nowrap;
}
</style>
