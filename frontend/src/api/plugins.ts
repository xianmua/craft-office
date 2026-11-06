export interface PluginCatalog {
  market: Array<{ id: string; name: string; version: string; description: string; file: string }>
  installed: Array<{ id: string; enabled: boolean }>
}

async function readError(res: Response): Promise<string> {
  try {
    const body = await res.json()
    if (body?.message) return `${body.message}（HTTP ${res.status}）`
  } catch {
    /* 非 JSON 响应 */
  }
  return `请求失败（HTTP ${res.status}）`
}

export async function fetchPlugins(): Promise<PluginCatalog> {
  const res = await fetch('/api/plugins')
  if (!res.ok) throw new Error(await readError(res))
  return (await res.json()) as PluginCatalog
}

async function post(op: 'install' | 'uninstall' | 'enable', body: Record<string, unknown>): Promise<void> {
  const res = await fetch(`/api/plugins/${op}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(await readError(res))
}

export function installPlugin(id: string): Promise<void> {
  return post('install', { id })
}

export function uninstallPlugin(id: string): Promise<void> {
  return post('uninstall', { id })
}

export function enablePlugin(id: string, enabled: boolean): Promise<void> {
  return post('enable', { id, enabled })
}

export async function fetchPluginFile(id: string): Promise<string> {
  const res = await fetch(`/api/plugins/file?id=${encodeURIComponent(id)}`)
  if (!res.ok) throw new Error(await readError(res))
  return res.text()
}
