export interface DocMeta {
  ext: string
  mime: string
  size: number | null
  supported: boolean
  /** 旧格式（doc/xls/ppt/csv）经后端 LibreOffice 自动转换而来 */
  converted: boolean
}

import { apiUrl, readError } from './base'

export async function fetchMeta(url: string, signal?: AbortSignal): Promise<DocMeta> {
  const res = await fetch(`${apiUrl('doc/meta')}?url=${encodeURIComponent(url)}`, { signal })
  if (!res.ok) throw new Error(await readError(res))
  return (await res.json()) as DocMeta
}

export async function fetchBinary(url: string, signal?: AbortSignal): Promise<ArrayBuffer> {
  const res = await fetch(`${apiUrl('doc/proxy')}?url=${encodeURIComponent(url)}`, { signal })
  if (!res.ok) throw new Error(await readError(res))
  return res.arrayBuffer()
}

/** 保存上报禁用标记：后端未配 SAVE_API_TOKEN（403）或不可达时置位，本次会话不再重试 */
let pushDisabled = false

/**
 * 保存时把最新文档字节上报到后端内存（拉模式）：
 * 第三方系统凭 SAVE_API_TOKEN 调 GET /api/save/latest{/file} 拉取，
 * 编辑器页面不做任何第三方回调注入。后端未启用时静默跳过。
 */
export async function pushLatestSave(name: string, saveAs: boolean, bytes: ArrayBuffer): Promise<void> {
  if (pushDisabled) return
  try {
    const res = await fetch(
      `${apiUrl('save/latest')}?name=${encodeURIComponent(name)}&saveAs=${saveAs}`,
      { method: 'POST', body: bytes },
    )
    if (res.status === 403 || res.status === 404) pushDisabled = true
  } catch {
    pushDisabled = true
  }
}
