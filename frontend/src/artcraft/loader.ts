/**
 * ArtCraft 内核适配层：这是整个前端**唯一**强耦合官方 API 的文件。
 * 官方 API 变更时只改这里，上层 OfficeView / EditorHost 不需要动。
 *
 * 期望的内核产物（见 vendor/artcraft/README.md）：
 *   /artcraft/artcraft.js      —— 暴露全局 window.ArtCraft = { WordCraft, GridCraft, DeckCraft }
 *   /artcraft/artcraft_bg.wasm —— 由 artcraft.js 自行 fetch 加载
 *
 * 每个 maker 需提供 create(options) 返回实例，实例需提供 destroy / save / setReadOnly。
 * 方法名做了别名兜底（save | export | exportAsync | toBlob），但契约缺一不可。
 */

export type DocKind = 'docx' | 'xlsx' | 'pptx'

export interface EditorInstance {
  destroy(): void
  save(): Promise<Blob>
  setReadOnly(value: boolean): void
}

export interface CreateOptions {
  container: HTMLElement
  binary: ArrayBuffer
  fileName: string
  readOnly: boolean
}

const KERNEL_SCRIPT = '/artcraft/artcraft.js'

const MAKERS: Record<DocKind, 'WordCraft' | 'GridCraft' | 'DeckCraft'> = {
  docx: 'WordCraft',
  xlsx: 'GridCraft',
  pptx: 'DeckCraft',
}

let sdkPromise: Promise<ArtCraftSdk | null> | null = null

/** 加载内核脚本；缺失或加载失败时返回 null（不抛错，交给降级预览兜底） */
export function loadKernel(): Promise<ArtCraftSdk | null> {
  if (sdkPromise) return sdkPromise

  sdkPromise = new Promise<ArtCraftSdk | null>((resolve) => {
    if (window.ArtCraft) {
      resolve(window.ArtCraft)
      return
    }

    const script = document.createElement('script')
    script.src = KERNEL_SCRIPT
    script.async = true
    script.onload = () => resolve(window.ArtCraft ?? null)
    script.onerror = () => {
      script.remove()
      resolve(null)
    }
    document.head.appendChild(script)
  })

  return sdkPromise
}

export async function hasKernelFor(kind: DocKind): Promise<boolean> {
  const sdk = await loadKernel()
  if (!sdk) return false
  const maker = sdk[MAKERS[kind]] as { create?: unknown } | undefined
  return typeof maker?.create === 'function'
}

export async function createEditor(
  kind: DocKind,
  options: CreateOptions,
): Promise<EditorInstance | null> {
  const sdk = await loadKernel()
  const maker = sdk?.[MAKERS[kind]] as { create?: (opts: Record<string, unknown>) => unknown } | undefined
  if (!maker || typeof maker.create !== 'function') return null

  const raw = (await Promise.resolve(
    maker.create({
      container: options.container,
      binary: options.binary,
      fileName: options.fileName,
      readOnly: options.readOnly,
      // 兼容部分内核直接吃 Uint8Array 的写法
      data: new Uint8Array(options.binary),
    }),
  )) as Record<string, unknown> | null

  if (!raw) throw new Error('内核 create() 未返回实例')

  const destroy = raw.destroy ?? raw.dispose
  const save = raw.save ?? raw.export ?? raw.exportAsync ?? raw.toBlob
  const setReadOnly = raw.setReadOnly ?? raw.setReadonly ?? raw.setEditable

  if (typeof destroy !== 'function' || typeof save !== 'function') {
    throw new Error('内核实例缺少 destroy / save 方法，请按官方 release 核对 API 后修改 artcraft/loader.ts')
  }

  return {
    destroy: () => void (destroy as () => void).call(raw),
    save: async () => {
      const out = await Promise.resolve((save as () => unknown).call(raw))
      if (out instanceof Blob) return out
      if (out instanceof ArrayBuffer) return new Blob([out])
      throw new Error('内核 save() 返回值不是 Blob / ArrayBuffer')
    },
    setReadOnly: (value: boolean) => {
      if (typeof setReadOnly === 'function') {
        void (setReadOnly as (v: boolean) => void).call(raw, value)
      }
    },
  }
}
