<template>
  <!-- 设置面板：通用 SidePanel 容器 + 手风琴设置项；引擎列表来自后端注册表（内置 + plugins/*/engine.json） -->
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
        v-for="e in aiEngines"
        :key="e.id"
        class="border rounded-lg overflow-hidden bg-white transition-colors"
        :class="expanded === e.id ? (aiEngine === e.id ? 'border-amber-400' : 'border-gray-300') : 'border-gray-200'"
      >
        <!-- 头部：标题 + 当前标记 + 展开/收起箭头 -->
        <button
          class="w-full flex items-center justify-between gap-2 border-0 bg-transparent p-3 text-left cursor-pointer transition-colors hover:bg-gray-50"
          @click="toggle(e.id)"
        >
          <span class="flex flex-col items-start gap-0.5 min-w-0">
            <span class="text-sm font-medium" :class="aiEngine === e.id ? 'text-amber-600' : 'text-gray-700'">
              {{ e.label }}{{ aiEngine === e.id ? ' · 当前' : '' }}
            </span>
            <span class="text-xs text-gray-400 leading-4 truncate w-full">{{ e.desc }}</span>
          </span>
          <svg
            class="w-4 h-4 shrink-0 text-gray-400 transition-transform"
            :class="expanded === e.id ? 'rotate-180' : ''"
            viewBox="0 0 16 16" fill="none"
          >
            <path d="M4 6l4 4 4-4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        </button>

        <!-- 展开区：apiKey / baseUrl / 模型 -->
        <div v-if="expanded === e.id" class="flex flex-col gap-3 px-3 pb-3 border-t border-gray-100 pt-3">
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">API Key（已配置：{{ masked[e.id] || '未配置' }}，留空不修改）</span>
            <input
              v-model="form[e.id].apiKey"
              type="password"
              autocomplete="off"
              placeholder="sk-..."
              class="appearance-none shadow-none h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">Base URL（留空用默认 {{ e.defaultBaseUrl }}）</span>
            <input
              v-model="form[e.id].baseUrl"
              type="text"
              :placeholder="e.defaultBaseUrl"
              class="appearance-none shadow-none h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-xs text-gray-500">模型（留空用默认 {{ e.defaultModel }}）</span>
            <input
              v-model="form[e.id].model"
              type="text"
              :placeholder="e.defaultModel"
              class="appearance-none shadow-none h-8 px-2 text-sm rounded-md border border-gray-300 outline-none focus:border-gray-500 transition-colors bg-white"
            />
          </label>

          <div class="flex items-center gap-2">
            <button
              class="h-8 px-4 rounded-md border-0 cursor-pointer text-sm text-white transition-colors"
              :class="saving ? 'bg-gray-300 cursor-default' : 'bg-blue-600 hover:bg-blue-700'"
              :disabled="saving"
              @click="save(e)"
            >{{ saving ? '保存中…' : '保存' }}</button>
            <span v-if="msg" class="text-xs" :class="msgOk ? 'text-green-600' : 'text-red-500'">{{ msg }}</span>
          </div>
        </div>
      </div>

      <p class="text-xs text-gray-400 m-0 leading-5">
        配置保存在后端 .env（API Key 不会回传明文），保存后即时生效，重启后仍有效。
        新增引擎：在 plugins/ 下建目录放 engine.json 即可被自动发现。
      </p>
    </div>
  </SidePanel>
</template>

<script setup lang="ts">
import { reactive, ref, watch } from 'vue'
import SidePanel from '@/components/SidePanel.vue'
import type { EngineConfigView } from '@/api/ai'
import { updateAiConfig } from '@/api/ai'
import { aiEngine, aiEngines, ensureEngines, setAiEngine } from '@/artcraft/aiEngine'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void }>()

// 每次打开面板强制刷新引擎列表（新装引擎插件立即可见）并回填表单
watch(
  () => props.visible,
  (v) => {
    if (v) {
      void ensureEngines(true).then((list) => {
        for (const e of list) hydrate(e.id)
      })
    }
  },
)

interface FormItem {
  apiKey: string
  baseUrl: string
  model: string
}
const expanded = ref('')
const masked = reactive<Record<string, string>>({})
const form = reactive<Record<string, FormItem>>({})
const saving = ref(false)
const msg = ref('')
const msgOk = ref(false)

function formOf(id: string): FormItem {
  if (!form[id]) form[id] = { apiKey: '', baseUrl: '', model: '' }
  return form[id]
}

/** 点头部：展开即选中该引擎；再点收起 */
function toggle(id: string) {
  if (expanded.value === id) {
    expanded.value = ''
    return
  }
  expanded.value = id
  setAiEngine(id)
  msg.value = ''
  hydrate(id)
}

/** 用后端当前配置填充展开区的回显（key 只回掩码，表单留空 = 不改） */
function hydrate(id: string) {
  const view = aiEngines.value.find((e) => e.id === id)
  if (!view) return
  masked[id] = view.apiKeyMasked ?? ''
  const f = formOf(id)
  f.baseUrl = view.baseUrl ?? ''
  f.model = view.model ?? ''
  f.apiKey = ''
}

async function save(e: EngineConfigView) {
  saving.value = true
  msg.value = ''
  try {
    const f = formOf(e.id)
    const patch: { apiKey?: string; baseUrl?: string; model?: string } = {}
    if (f.apiKey.trim()) patch.apiKey = f.apiKey.trim()
    if (f.baseUrl.trim()) patch.baseUrl = f.baseUrl.trim()
    if (f.model.trim()) patch.model = f.model.trim()
    const result = await updateAiConfig(e.id, patch)
    masked[e.id] = result.apiKeyMasked
    f.apiKey = ''
    msg.value = `已保存：${result.model} @ ${result.baseUrl}`
    msgOk.value = true
    hydrate(e.id)
  } catch (err) {
    msg.value = err instanceof Error ? err.message : String(err)
    msgOk.value = false
  } finally {
    saving.value = false
  }
}

// 面板打开时刷新当前展开项的回显
watch(
  () => expanded.value,
  (v) => {
    if (v) hydrate(v)
  },
)
</script>
