<template>
  <!-- 设置面板：通用 SidePanel 容器 + 手风琴设置项；后续新增设置往小节里加即可 -->
  <SidePanel
    :visible="visible"
    title="设置"
    @minimize="emit('update:visible', false)"
    @close="emit('update:visible', false)"
  >
    <div class="flex flex-col p-4 gap-4 overflow-y-auto">
      <h3 class="text-xs font-medium text-gray-400 m-0 uppercase tracking-wide">AI 引擎</h3>

      <!-- 手风琴：点击头部展开该引擎配置并设为当前引擎 -->
      <div
        v-for="opt in ENGINE_OPTIONS"
        :key="opt.value"
        class="border rounded-lg overflow-hidden bg-white transition-colors"
        :class="expanded === opt.value ? (aiEngine === opt.value ? 'border-amber-400' : 'border-gray-300') : 'border-gray-200'"
      >
        <!-- 头部：标题 + 当前标记 + 展开/收起箭头 -->
        <button
          class="w-full flex items-center justify-between gap-2 border-0 bg-transparent p-3 text-left cursor-pointer transition-colors hover:bg-gray-50"
          @click="toggle(opt.value)"
        >
          <span class="flex flex-col items-start gap-0.5 min-w-0">
            <span class="text-sm font-medium" :class="aiEngine === opt.value ? 'text-amber-600' : 'text-gray-700'">
              {{ opt.label }}{{ aiEngine === opt.value ? ' · 当前' : '' }}
            </span>
            <span class="text-xs text-gray-400 leading-4 truncate w-full">{{ opt.desc }}</span>
          </span>
          <svg
            class="w-4 h-4 shrink-0 text-gray-400 transition-transform"
            :class="expanded === opt.value ? 'rotate-180' : ''"
            viewBox="0 0 16 16" fill="none"
          >
            <path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>

        <!-- 展开区：apiKey / baseUrl / 模型 -->
        <div v-if="expanded === opt.value" class="flex flex-col gap-3 px-3 pb-3 border-t border-gray-100 pt-3">
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">
              {{ opt.value === 'dsh' ? 'API Key（本插件无鉴权，可留空）' : `API Key（已配置：${keyMasked(opt.value) || '未配置'}，留空不修改）` }}
            </span>
            <input
              v-model="form[opt.value].apiKey"
              type="password"
              autocomplete="off"
              placeholder="sk-..."
              class="h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">Base URL（留空用默认 {{ DEFAULT_BASE[opt.value] }}）</span>
            <input
              v-model="form[opt.value].baseUrl"
              type="text"
              :placeholder="DEFAULT_BASE[opt.value]"
              class="h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">模型（可下拉选择或手输）</span>
            <input
              v-model="form[opt.value].model"
              type="text"
              :placeholder="DEFAULT_MODEL[opt.value]"
              class="h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
            <span v-if="modelList.length" class="flex flex-wrap gap-1 mt-1">
              <button
                v-for="m in modelList"
                :key="m"
                class="text-xs px-2 py-0.5 rounded border cursor-pointer transition-colors"
                :class="form[opt.value].model === m ? 'border-amber-400 bg-amber-50 text-amber-600' : 'border-gray-200 text-gray-500 hover:border-gray-300'"
                @click="form[opt.value].model = m"
              >{{ m }}</button>
            </span>
            <button
              class="self-start text-xs border-0 bg-transparent p-0 cursor-pointer text-blue-600 hover:text-blue-700 mt-0.5"
              @click="loadModels(opt.value)"
            >{{ loadingModels ? '获取中…' : modelList.length ? '刷新模型列表' : '获取可用模型列表' }}</button>
          </label>

          <div class="flex items-center gap-2">
            <button
              class="h-8 px-4 rounded-md border-0 cursor-pointer text-sm text-white transition-colors"
              :class="saving ? 'bg-gray-300 cursor-default' : 'bg-blue-600 hover:bg-blue-700'"
              :disabled="saving"
              @click="save(opt.value)"
            >{{ saving ? '保存中…' : '保存' }}</button>
            <span v-if="msg" class="text-xs" :class="msgOk ? 'text-green-600' : 'text-red-500'">{{ msg }}</span>
          </div>
        </div>
      </div>

      <p class="text-xs text-gray-400 m-0 leading-5">
        配置保存在后端 .env（API Key 不会回传明文），保存后即时生效，重启后仍有效。
      </p>
    </div>
  </SidePanel>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import SidePanel from '@/components/SidePanel.vue'
import type { AiConfigView, AiEngine } from '@/api/ai'
import { fetchAiConfig, fetchAiModels, updateAiConfig } from '@/api/ai'
import { aiEngine, setAiEngine } from '@/artcraft/aiEngine'

defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

const ENGINE_OPTIONS: Array<{ value: AiEngine; label: string; desc: string }> = [
  { value: 'deepseek', label: 'LLM AI', desc: 'DeepSeek 直连（服务端网关）' },
  { value: 'dsh', label: 'DSH AI', desc: '本机 dsh-apis-plugin，OpenAI 兼容端点' },
]
const DEFAULT_BASE: Record<AiEngine, string> = {
  deepseek: 'https://api.deepseek.com',
  dsh: 'http://127.0.0.1:3088/v1',
}
const DEFAULT_MODEL: Record<AiEngine, string> = { deepseek: 'deepseek-chat', dsh: 'id456/glm-5.3-flash' }

const expanded = ref<AiEngine | ''>('')
const maskedKeys = reactive<Record<AiEngine, string>>({ deepseek: '', dsh: '' })
const form = reactive<Record<AiEngine, { apiKey: string; baseUrl: string; model: string }>>({
  deepseek: { apiKey: '', baseUrl: '', model: '' },
  dsh: { apiKey: '', baseUrl: '', model: '' },
})
const modelList = ref<string[]>([])
const loadingModels = ref(false)
const saving = ref(false)
const msg = ref('')
const msgOk = ref(false)

function keyMasked(engine: AiEngine): string {
  return maskedKeys[engine]
}

/** 点头部：展开即选中该引擎；再点收起 */
function toggle(engine: AiEngine) {
  if (expanded.value === engine) {
    expanded.value = ''
    return
  }
  expanded.value = engine
  setAiEngine(engine)
  modelList.value = []
  msg.value = ''
  void hydrate(engine)
}

/** 用后端当前配置填充展开区的回显（key 只回掩码，表单留空 = 不改） */
async function hydrate(engine: AiEngine) {
  try {
    const cfg: AiConfigView = await fetchAiConfig()
    const view = cfg[engine]
    maskedKeys[engine] = view.apiKeyMasked ?? ''
    form[engine].baseUrl = view.baseUrl ?? ''
    form[engine].model = view.model ?? ''
    form[engine].apiKey = ''
  } catch {
    maskedKeys[engine] = ''
  }
}

async function loadModels(engine: AiEngine) {
  loadingModels.value = true
  msg.value = ''
  try {
    // 列表拉取用表单里的 model/key 之前需先保存；直接用当前后端配置
    modelList.value = await fetchAiModels(engine)
  } catch (err) {
    msg.value = `获取模型列表失败：${err instanceof Error ? err.message : err}`
    msgOk.value = false
  } finally {
    loadingModels.value = false
  }
}

async function save(engine: AiEngine) {
  saving.value = true
  msg.value = ''
  try {
    const f = form[engine]
    const patch: { apiKey?: string; baseUrl?: string; model?: string } = {}
    if (f.apiKey.trim()) patch.apiKey = f.apiKey.trim()
    if (f.baseUrl.trim()) patch.baseUrl = f.baseUrl.trim()
    if (f.model.trim()) patch.model = f.model.trim()
    const result = await updateAiConfig(engine, patch)
    maskedKeys[engine] = result.apiKeyMasked
    form[engine].apiKey = ''
    msg.value = `已保存：${result.model} @ ${result.baseUrl}`
    msgOk.value = true
  } catch (err) {
    msg.value = err instanceof Error ? err.message : String(err)
    msgOk.value = false
  } finally {
    saving.value = false
  }
}

// 面板打开时刷新当前引擎的回显
watch(
  () => expanded.value,
  (v) => {
    if (v) void hydrate(v)
  },
)
</script>
