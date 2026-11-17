/**
 * AI 引擎全局状态（内置 LLM + 插件引擎，列表来自后端引擎注册表）：
 * 扇形菜单、助手输入区、设置面板三处共享，localStorage 记住选择。
 */
import { ref } from 'vue'
import type { AiConfigView, AiEngine } from '@/api/ai'
import { fetchAiConfig } from '@/api/ai'

const KEY = 'artcraft-ai-engine'

export const aiEngine = ref<AiEngine>(localStorage.getItem(KEY) ?? 'deepseek')

/** 引擎列表（后端注册表视图），懒加载一次全局共享 */
export const aiEngines = ref<AiConfigView>([])

let enginesPromise: Promise<AiConfigView> | null = null

/** 拉取引擎列表（默认缓存一次；force=true 强制刷新，设置面板打开时用） */
export function ensureEngines(force = false): Promise<AiConfigView> {
  if (force) enginesPromise = null
  if (!enginesPromise) {
    enginesPromise = fetchAiConfig()
      .then((list) => {
        aiEngines.value = list
        // localStorage 里的引擎已被卸载时回落到内置 deepseek
        if (!list.some((e) => e.id === aiEngine.value)) aiEngine.value = 'deepseek'
        return list
      })
      .catch(() => [])
  }
  return enginesPromise
}

export function setAiEngine(e: AiEngine) {
  aiEngine.value = e
  localStorage.setItem(KEY, e)
}
