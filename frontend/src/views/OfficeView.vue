<template>
  <div class="page-shell bg-gray-50">
    <main class="relative flex-1 min-h-0">
      <div v-if="loading" class="absolute inset-0 z-10 flex-center flex-col gap-3 bg-white/85">
        <t-loading size="small" />
        <span class="text-sm text-muted">{{ loadingHint }}</span>
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
        <svg class="w-9 h-9 text-orange-500" viewBox="0 0 24 24" fill="currentColor">
          <path d="M12 2a10 10 0 1 1 0 20 10 10 0 0 1 0-20zm0 5a1 1 0 0 1 1 1v5a1 1 0 1 1-2 0V8a1 1 0 0 1 1-1zm0 9.2a1.2 1.2 0 1 1 0 2.4 1.2 1.2 0 0 1 0-2.4z" />
        </svg>
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
        />
        <FallbackPreview v-else-if="preview" :model="preview" />

        <!-- 外壳 dock：竖条+扇形菜单为唯一入口；助手 / 设置面板共用 SidePanel 容器 -->
        <template v-if="shellAssistant">
          <FanDock
            v-if="!assistantOpen && !settingsOpen"
            :busy="assistantBusy"
            :engines="dockEngines"
            @action="onDockAction"
            @expand="assistantOpen = true"
          />
          <AssistantPanel
            :visible="assistantOpen"
            :kernel="editorKernel"
            :bridge="assistantBridge"
            @update:visible="assistantOpen = $event"
            @busy="assistantBusy = $event"
          />
          <SettingsPanel v-model:visible="settingsOpen" @changed="refreshEnabledPlugins" />
        </template>

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

    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, nextTick, ref } from "vue";
import { MessagePlugin } from "tdesign-vue-next";
import KernelFrame from "@/components/KernelFrame.vue";
import FallbackPreview from "@/components/FallbackPreview.vue";
import FanDock from "@/components/FanDock.vue";
import AssistantPanel from "@/components/AssistantPanel.vue";
import SettingsPanel from "@/components/SettingsPanel.vue";
import { setAiEngine, aiEngines } from "@/artcraft/aiEngine";
import { ensureCjkFont, shellKernelIds, type KernelBridge } from "@/artcraft/shellAssistant";
import { useDocSource, fileNameOf } from "@/composables/useDocSource";
import { fetchBinary, fetchMeta, pushLatestSave } from "@/api/doc";
import { fetchPlugins } from "@/api/plugins";
import { KERNEL_OF, type KernelSavePayload } from "@/artcraft/loader";
import { createHostRelay } from "@/artcraft/hostRelay";
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

/** 代理正文实际扩展名：旧格式自动转换后为转换产物扩展名（meta.ext），用于打开文件名 */
const docExt = ref("");
const loadingHint = ref("正在通过后端代理拉取文档…");

/** 打开内核用的文件名：扩展名跟随代理正文（转换后 a.doc → a.docx），内核按扩展名分型 */
const openName = computed(() => {
  const raw = fileName.value || "document";
  return docExt.value ? `${raw.replace(/\.[^.]*$/, "")}.${docExt.value}` : raw;
});

/** 第三方嵌入中继：被嵌且 ?parentOrigin= 白名单声明时，内核事件向上转发（协议见 hostRelay.ts） */
const relay = createHostRelay((name, bytes) => kernelRef.value?.openDoc(name, bytes));

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
    enabledIds.value = enabled;
    dock.value = catalog.market.map((p) => ({
      id: p.id,
      name: p.name,
      enabled: enabled.has(p.id),
    }));
  } catch {
    enabledIds.value = new Set();
    dock.value = [];
  }
}

/** 外壳 AI 助手 gate：内核在注册表（kernels.json 数据驱动）+ ai-assistant 插件启用 */
const shellAssistant = computed(
  () =>
    shellKernelIds.value.includes(editorKernel.value) &&
    enabledIds.value.has("ai-assistant"),
);
const assistantOpen = ref(false);
const enabledIds = ref<Set<string>>(new Set());
const settingsOpen = ref(false);
const assistantBusy = ref(false);

/** 扇形菜单引擎扇区：引擎注册表驱动（manifest.json engine 节插件即增），LLM/DSH/未来引擎共用同一映射 */
const dockEngines = computed(() => aiEngines.value.map((e) => ({ id: e.id, label: e.label })));

/** 竖条扇形菜单动作分发：选引擎并开助手 / 保存 / 设置 */
function onDockAction(key: string) {
  if (key === "save") {
    // 走内核 file.save 命令，宿主桥会收到保存事件并触发下载
    assistantBridge.value?.("execute", "file.save", {}).catch(() => {});
  } else if (key === "settings") {
    settingsOpen.value = true;
  } else {
    // 其余 key = 引擎 id
    setAiEngine(key);
    assistantOpen.value = true;
  }
}

/** 页面命令 API 桥：转交 KernelFrame（同源 iframe contentWindow.window.<app>） */
const kernelBridge = computed<KernelBridge | null>(() =>
  kernelRef.value ? (method, ...args) => kernelRef.value!.call(method, ...args) : null,
);
/** AI 助手桥：仅在助手 gate 通过时可用（字体安装等基础功能走 kernelBridge，不依赖助手开关） */
const assistantBridge = computed<KernelBridge | null>(() =>
  shellAssistant.value
    ? (method, ...args) => kernelRef.value!.call(method, ...args)
    : null,
);

function onSocketClick(item: DockItem) {
  if (!item.enabled) MessagePlugin.warning(`「${item.name}」未开通`);
}

const fileName = computed(() => (remoteUrl.value ? fileNameOf(remoteUrl.value) : ""));

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
  docExt.value = "";

  try {
    const meta = await fetchMeta(remoteUrl.value);
    if (!meta.supported) throw new Error(`后端判定该格式不受支持：.${meta.ext}`);
    docExt.value = meta.ext;
    loadingHint.value = meta.converted
      ? "旧格式文档转换中（doc/xls/ppt → OOXML）…"
      : "正在通过后端代理拉取文档…";

    const bytes = await fetchBinary(remoteUrl.value);
    binary.value = bytes;

    const docKind = kind.value === "unknown" ? undefined : kind.value;
    const kernelName = docKind ? KERNEL_OF[docKind] : undefined;

    // 不再依赖 /api/health：按扩展名直接进内核模式，内核启动失败由 onKernelFailed 降级预览
    if (kernelName) {
      // 内核模式：iframe 就绪后由 onKernelReady 投递文档字节
      editorKernel.value = kernelName;
      mode.value = "editor";
      await nextTick();
    } else {
      // pdf 有专属内核必命中上分支，这里只剩 OOXML 三件套 / unknown
      preview.value = inspectOoxml(docKind === "pdf" ? "docx" : (docKind ?? "docx"), new Uint8Array(bytes));
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

async function onKernelReady() {
  relay.toParent({ type: "artcraft:ready" });
  // wasm 进程无系统中文字体，先装完 CJK 兜底字体再开文档：
  // deckcraft 已布局文本不会随新装字体重排，边开边装会长时间显示方块（装完才由缩放往返纠正）
  await ensureCjkFont(editorKernel.value, kernelBridge.value);
  if (!binary.value) return;
  kernelRef.value?.openDoc(openName.value || "document.docx", binary.value);
}

async function onKernelSave(payload: KernelSavePayload) {
  // ① 嵌入模式：保存字节 postMessage 转交第三方父页面持久化
  let handled = false;
  if (relay.active) {
    relay.toParent({ type: "artcraft:save", name: payload.name, saveAs: payload.saveAs, bytes: payload.bytes });
    handled = true;
  }
  // ② 拉模式上报：字节报到后端内存，第三方凭 SAVE_API_TOKEN 调 /api/save/latest 拉取；
  //    后端未启用（未配 token）时静默跳过，不阻塞保存流程
  void pushLatestSave(payload.name, payload.saveAs, payload.bytes);
  if (handled) {
    dirty.value = false;
    return;
  }
  // ③ 默认：本地下载
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
  relay.toParent({ type: "artcraft:dirty", dirty: value });
}

function onKernelFailed(message: string) {
  relay.toParent({ type: "artcraft:failed", error: message });
  MessagePlugin.error(`内核启动失败：${message}`);
  // pdf 非 OOXML 无降级预览，失败直接展示错误页
  if (binary.value && kind.value !== "pdf") {
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

onBeforeUnmount(() => relay.dispose());
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
