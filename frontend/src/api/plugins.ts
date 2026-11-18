export interface PluginCatalog {
  market: Array<{ id: string; name: string; version: string; description: string; source: string }>
  installed: Array<{ id: string; enabled: boolean }>
}

import { apiUrl, readError } from './base'

export async function fetchPlugins(): Promise<PluginCatalog> {
  const res = await fetch(apiUrl('plugins'))
  if (!res.ok) throw new Error(await readError(res))
  return (await res.json()) as PluginCatalog
}

export async function enablePlugin(id: string, enabled: boolean): Promise<void> {
  const res = await fetch(apiUrl('plugins/enable'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, enabled }),
  })
  if (!res.ok) throw new Error(await readError(res))
}
