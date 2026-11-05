import { ref } from 'vue'

export type DocKind = 'docx' | 'xlsx' | 'pptx' | 'unknown'

const EXT_MAP: Record<string, DocKind> = {
  docx: 'docx',
  doc: 'docx',
  xlsx: 'xlsx',
  xls: 'xlsx',
  csv: 'xlsx',
  pptx: 'pptx',
  ppt: 'pptx',
}

export function kindOfUrl(raw: string): DocKind {
  const clean = raw.split('#')[0].split('?')[0]
  const ext = clean.slice(clean.lastIndexOf('.') + 1).toLowerCase()
  return EXT_MAP[ext] ?? 'unknown'
}

export function fileNameOf(raw: string): string {
  const clean = raw.split('#')[0].split('?')[0]
  const name = clean.slice(clean.lastIndexOf('/') + 1)
  try {
    return decodeURIComponent(name) || '在线文档'
  } catch {
    return name || '在线文档'
  }
}

export function useDocSource() {
  const remoteUrl = ref('')
  const kind = ref<DocKind>('unknown')
  const errorText = ref('')

  /** 解析 location.search 中的 url；返回是否可用 */
  function parse(query?: string): boolean {
    remoteUrl.value = ''
    kind.value = 'unknown'
    errorText.value = ''

    const sp = new URLSearchParams(query ?? location.search)
    const raw = (sp.get('url') ?? '').trim()
    if (!raw) {
      errorText.value = '缺少 url 参数，示例：/office?url=https://cdn.example.com/a.docx'
      return false
    }

    let parsed: URL
    try {
      parsed = new URL(raw)
    } catch {
      errorText.value = 'url 参数不是合法的绝对地址'
      return false
    }

    if (!['http:', 'https:'].includes(parsed.protocol)) {
      errorText.value = `不支持的协议：${parsed.protocol}，仅支持 http / https`
      return false
    }

    const detected = kindOfUrl(raw)
    if (detected === 'unknown') {
      const ext = raw.split('#')[0].split('?')[0].split('.').pop() ?? ''
      errorText.value = `暂不支持的文件类型：.${ext}（当前支持 docx / xlsx / pptx）`
      return false
    }

    remoteUrl.value = raw
    kind.value = detected
    return true
  }

  return { remoteUrl, kind, errorText, parse }
}
