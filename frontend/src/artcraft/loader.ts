/**
 * ArtCraft 内核适配层：这是整个前端**唯一**强耦合官方内核协议的文件。
 * 内核协议变更时只改这里，上层 OfficeView / KernelFrame 不需要动。
 *
 * 内核产物（trunk build 输出，见 .kernel-src/wordcraft-main/apps/wordcraft-web）：
 *   /wordcraft/index.html     —— 完整 WordCraft 应用（eframe wasm）
 *
 * 集成协议：postMessage 宿主桥（iframe 带 ?host=parent，源码见
 * .kernel-src/wordcraft-main/apps/wordcraft-web/src/host.rs）：
 *   内核 → 宿主：ready{version} / save{name,saveAs,bytes} / dirty{dirty} / failed{error}
 *   宿主 → 内核：open{name,bytes}（bytes 为 ArrayBuffer / Uint8Array）
 * 仅同源父页面有效；/office 与 /wordcraft 同源，天然满足。
 */

import type { DocKind } from '@/composables/useDocSource'

export type { DocKind }

/** 各格式对应的内核；gridcraft / deckcraft 产物部署后在此扩展（xlsx → gridcraft、pptx → deckcraft） */
export const KERNEL_OF: Partial<Record<DocKind, string>> = {
  docx: 'wordcraft',
}

/** 内核 iframe 地址；?host=parent 启用 postMessage 宿主桥 */
export function kernelFrameUrl(kernel: string): string {
  return `/${kernel}/index.html?host=parent`
}

export interface KernelSavePayload {
  name: string
  saveAs: boolean
  bytes: ArrayBuffer
}

export type KernelEvent =
  | { type: 'ready' }
  | { type: 'failed'; message: string }
  | { type: 'save'; payload: KernelSavePayload }
  | { type: 'dirty'; value: boolean }
  | { type: 'plugins' }

/** 解析内核发来的消息；非该内核的消息返回 null */
export function parseKernelMessage(kernel: string, data: unknown): KernelEvent | null {
  if (!data || typeof data !== 'object') return null
  const msg = data as Record<string, unknown>
  const type = typeof msg.type === 'string' ? msg.type : ''
  switch (type) {
    case `${kernel}:ready`:
      return { type: 'ready' }
    case `${kernel}:plugins`:
      return { type: 'plugins' }
    case `${kernel}:failed`:
      return { type: 'failed', message: typeof msg.error === 'string' ? msg.error : '内核启动失败' }
    case `${kernel}:dirty`:
      return { type: 'dirty', value: msg.dirty === true }
    case `${kernel}:save`: {
      const bytes = toBytes(msg.bytes)
      if (!bytes) return null
      return {
        type: 'save',
        payload: {
          name: typeof msg.name === 'string' && msg.name.trim() ? msg.name : 'document',
          saveAs: msg.saveAs === true,
          bytes,
        },
      }
    }
    default:
      return null
  }
}

/** 宿主 → 内核的 open 消息体（bytes 走结构化克隆，不 transfer，避免源缓冲被 detach） */
export function openMessage(name: string, bytes: ArrayBuffer): { type: string; name: string; bytes: ArrayBuffer } {
  return { type: 'wordcraft:open', name, bytes }
}

/** bytes 字段：ArrayBuffer 或 Uint8Array */
function toBytes(value: unknown): ArrayBuffer | null {
  if (value instanceof ArrayBuffer) return value
  if (value instanceof Uint8Array) {
    return value.buffer.slice(value.byteOffset, value.byteOffset + value.byteLength) as ArrayBuffer
  }
  return null
}
