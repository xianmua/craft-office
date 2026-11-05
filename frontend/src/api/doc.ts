export interface DocMeta {
  ext: string
  mime: string
  size: number | null
  supported: boolean
}

/** 后端统一错误体：{ code, message } */
async function readError(res: Response): Promise<string> {
  try {
    const body = await res.json()
    if (body?.message) return `${body.message}（HTTP ${res.status}）`
  } catch {
    /* 非 JSON 响应 */
  }
  return `请求失败（HTTP ${res.status}）`
}

export async function fetchMeta(url: string, signal?: AbortSignal): Promise<DocMeta> {
  const res = await fetch(`/api/doc/meta?url=${encodeURIComponent(url)}`, { signal })
  if (!res.ok) throw new Error(await readError(res))
  return (await res.json()) as DocMeta
}

export async function fetchBinary(url: string, signal?: AbortSignal): Promise<ArrayBuffer> {
  const res = await fetch(`/api/doc/proxy?url=${encodeURIComponent(url)}`, { signal })
  if (!res.ok) throw new Error(await readError(res))
  return res.arrayBuffer()
}

/** 走代理的下载地址（强制 attachment，避免签名直链暴露给浏览器） */
export function downloadUrl(url: string): string {
  return `/api/doc/proxy?url=${encodeURIComponent(url)}&download=1`
}

export async function fetchHealth(): Promise<{ editor_kernel_ready: boolean; max_bytes: number }> {
  const res = await fetch('/api/health')
  if (!res.ok) throw new Error(await readError(res))
  return res.json()
}
