/**
 * 子路径部署适配：构建/开发时用 ARTCRAFT_BASE 指定前缀（如 /office/），
 * 代码中所有对后端 API 与内核静态页的引用必须经由这里拼接，禁止写根绝对路径。
 */

/** 拼接部署前缀（import.meta.env.BASE_URL 由 vite 注入，保证以 / 结尾） */
function withBase(path: string): string {
  return `${import.meta.env.BASE_URL}${path.replace(/^\//, '')}`
}

/** 后端 API 地址：apiUrl('doc/meta') → {base}api/doc/meta */
export function apiUrl(path: string): string {
  return withBase(`api/${path.replace(/^\//, '')}`)
}

/** 内核静态页地址：kernelUrl('wordcraft/index.html') → {base}wordcraft/index.html */
export function kernelUrl(path: string): string {
  return withBase(path)
}

/** 后端统一错误体：{ code, message } → 提示文本 */
export async function readError(res: Response): Promise<string> {
  try {
    const body = await res.json()
    if (body?.message) return `${body.message}（HTTP ${res.status}）`
  } catch {
    /* 非 JSON 响应 */
  }
  return `请求失败（HTTP ${res.status}）`
}
