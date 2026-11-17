/**
 * AI 引擎全局状态（LLM=DeepSeek 直连 / DSH=本机 Harness）：
 * 扇形菜单、助手输入区、设置面板三处共享，localStorage 记住选择。
 */
import { ref } from 'vue'
import type { AiEngine } from '@/api/ai'

const KEY = 'artcraft-ai-engine'

export const aiEngine = ref<AiEngine>(localStorage.getItem(KEY) === 'dsh' ? 'dsh' : 'deepseek')

export function setAiEngine(e: AiEngine) {
  aiEngine.value = e
  localStorage.setItem(KEY, e)
}
