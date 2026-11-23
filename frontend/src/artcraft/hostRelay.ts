/**
 * 第三方嵌入中继（host relay）：/office 被外部页面 iframe 嵌入时，把内核事件向上
 * 转发给父页面，把父页面的 open 指令向下转发给内核。协议（type 前缀 artcraft:）：
 *   中继 → 父页面：artcraft:ready{} / artcraft:save{name,saveAs,bytes} /
 *           artcraft:dirty{dirty} / artcraft:failed{error}
 *   父页面 → 中继：artcraft:open{name,bytes}（内核未就绪时 KernelFrame 自动排队）
 * 安全：仅当「确实被嵌入」且 URL 带 ?parentOrigin=（逗号分隔白名单）时启用；
 * 收发都按白名单校验 origin，文档字节不发给名单之外；未启用时行为不变（本地下载）。
 */

export interface RelayEvent {
  type: 'artcraft:ready' | 'artcraft:save' | 'artcraft:dirty' | 'artcraft:failed'
  name?: string
  saveAs?: boolean
  bytes?: ArrayBuffer
  dirty?: boolean
  error?: string
}

export interface HostRelay {
  active: boolean
  toParent: (event: RelayEvent) => void
  dispose: () => void
}

export function createHostRelay(onOpen: (name: string, bytes: ArrayBuffer) => void): HostRelay {
  const allowed = (new URLSearchParams(location.search).get('parentOrigin') ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean)
  const active = allowed.length > 0 && window.self !== window.top

  function toParent(event: RelayEvent) {
    if (!active) return
    for (const origin of allowed) window.parent.postMessage(event, origin)
  }

  function onMessage(event: MessageEvent) {
    if (!active || !allowed.includes(event.origin)) return
    const data = event.data as Record<string, unknown> | null
    if (!data || data.type !== 'artcraft:open') return
    if (!(data.bytes instanceof ArrayBuffer)) return
    const name = typeof data.name === 'string' && data.name.trim() ? data.name : 'document.docx'
    onOpen(name, data.bytes)
  }

  window.addEventListener('message', onMessage)
  return {
    active,
    toParent,
    dispose: () => window.removeEventListener('message', onMessage),
  }
}
