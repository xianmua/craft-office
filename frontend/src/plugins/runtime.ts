/**
 * 插件运行时：拉取已安装插件的 JS，用受限的 hostApi 执行（不进 iframe、不碰内核）。
 * 插件能做的：读文档文本、调 AI、注册命令（悬浮球菜单里出现）、弹提示/对话框。
 */

export interface PluginCommand {
  id: string
  title: string
  run: () => void | Promise<void>
}

export interface HostApi {
  registerCommand(cmd: PluginCommand): void
  docText(): string
  aiChat(messages: Array<{ role: string; content: string }>): Promise<string>
  toast(message: string): void
  dialog(title: string, content: string): void
}

export interface LoadedPlugin {
  id: string
  name: string
  version: string
  commands: PluginCommand[]
  error?: string
}

interface MarketEntry {
  id: string
  name: string
  version: string
  description: string
  file: string
}

export interface PluginCatalog {
  market: MarketEntry[]
  installed: string[]
}

import { fetchPluginFile, fetchPlugins, installPlugin, uninstallPlugin } from '@/api/plugins'

/** 拉市场目录 */
export async function loadCatalog(): Promise<PluginCatalog> {
  return fetchPlugins()
}

export async function install(id: string): Promise<void> {
  await installPlugin(id)
}

export async function uninstall(id: string): Promise<void> {
  await uninstallPlugin(id)
}

/** 加载并执行所有已安装插件，返回每个插件的加载结果 */
export async function runPlugins(api: Omit<HostApi, 'registerCommand'>): Promise<LoadedPlugin[]> {
  const catalog = await loadCatalog()
  const out: LoadedPlugin[] = []
  for (const id of catalog.installed) {
    const entry = catalog.market.find((m) => m.id === id)
    if (!entry) continue
    const plugin: LoadedPlugin = { id, name: entry.name, version: entry.version, commands: [] }
    try {
      const code = await fetchPluginFile(id)
      const withRegister: HostApi = {
        ...api,
        registerCommand: (cmd: PluginCommand) => {
          if (cmd && cmd.id && cmd.title && typeof cmd.run === 'function') {
            plugin.commands.push(cmd)
          }
        },
      }
      // 插件是本机自装内容，等同扩展代码；IIFE 里只能看到 hostApi 一个入口
      // eslint-disable-next-line no-new-func
      const factory = new Function('hostApi', `"use strict";\n${code}`)
      factory(withRegister)
    } catch (err) {
      plugin.error = err instanceof Error ? err.message : '插件执行失败'
    }
    out.push(plugin)
  }
  return out
}
