/**
 * 降级预览：在内核未接入时，直接在浏览器解 OOXML 压缩包，给出结构化预览。
 * 只做「看得见内容」这一件事，不承担编辑 / 排版还原，也不上传任何内容到服务端。
 */
import { unzipSync, strFromU8 } from 'fflate'

export type DocKind = 'docx' | 'xlsx' | 'pptx'

export interface SheetPreview {
  name: string
  rows: string[][]
  truncated: boolean
}

export interface SlidePreview {
  index: number
  texts: string[]
}

export interface PreviewModel {
  kind: DocKind
  entryCount: number
  paragraphs?: string[]
  sheets?: SheetPreview[]
  slides?: SlidePreview[]
  /** 说明本次预览被裁剪 / 降级的点 */
  notes: string[]
}

const MAX_PARAGRAPHS = 500
const MAX_ROWS = 80
const MAX_COLS = 20
const MAX_SLIDE_TEXTS = 40

function parseXml(text: string): Document {
  const doc = new DOMParser().parseFromString(text, 'application/xml')
  if (doc.getElementsByTagName('parsererror').length > 0) {
    throw new Error('OOXML 内部 XML 解析失败')
  }
  return doc
}

function textOf(node: Element, tag: string): string {
  return Array.from(node.getElementsByTagName(tag))
    .map((n) => n.textContent ?? '')
    .join('')
}

function readEntry(entries: Record<string, Uint8Array>, path: string): string | null {
  const file = entries[path]
  return file ? strFromU8(file) : null
}

function previewDocx(entries: Record<string, Uint8Array>, notes: string[]): PreviewModel['paragraphs'] {
  const xml = readEntry(entries, 'word/document.xml')
  if (!xml) {
    notes.push('未找到 word/document.xml，可能不是标准 docx')
    return []
  }
  const doc = parseXml(xml)
  const paragraphs = Array.from(doc.getElementsByTagName('w:p'))
    .map((p) => textOf(p, 'w:t').replace(/\s+/g, ' ').trim())
    .filter((s) => s.length > 0)

  if (paragraphs.length > MAX_PARAGRAPHS) {
    notes.push(`正文共 ${paragraphs.length} 段，预览仅显示前 ${MAX_PARAGRAPHS} 段`)
  }
  return paragraphs.slice(0, MAX_PARAGRAPHS)
}

export function colIndex(ref: string): number {
  const letters = ref.replace(/[^A-Z]/g, '')
  let n = 0
  for (const ch of letters) n = n * 26 + (ch.charCodeAt(0) - 64)
  return Math.max(0, n - 1)
}

export function rowIndex(ref: string): number {
  const digits = ref.replace(/[^0-9]/g, '')
  return Math.max(0, Number.parseInt(digits || '1', 10) - 1)
}

function previewXlsx(entries: Record<string, Uint8Array>, notes: string[]): SheetPreview[] {
  const workbookXml = readEntry(entries, 'xl/workbook.xml')
  const names = workbookXml
    ? Array.from(parseXml(workbookXml).getElementsByTagName('sheet')).map(
        (s) => s.getAttribute('name') ?? 'Sheet',
      )
    : []

  const sharedXml = readEntry(entries, 'xl/sharedStrings.xml')
  const shared = sharedXml
    ? Array.from(parseXml(sharedXml).getElementsByTagName('si')).map((si) => textOf(si, 't'))
    : []

  const sheetFiles = Object.keys(entries)
    .filter((k) => /^xl\/worksheets\/sheet\d+\.xml$/.test(k))
    .sort((a, b) => {
      const na = Number(a.match(/sheet(\d+)\.xml$/)?.[1] ?? 0)
      const nb = Number(b.match(/sheet(\d+)\.xml$/)?.[1] ?? 0)
      return na - nb
    })

  if (sheetFiles.length === 0) {
    notes.push('未找到工作表 XML，可能不是标准 xlsx')
    return []
  }

  return sheetFiles.slice(0, 5).map((path, i) => {
    const xml = readEntry(entries, path)
    const rows: string[][] = []
    let truncated = false

    if (xml) {
      for (const cell of Array.from(parseXml(xml).getElementsByTagName('c'))) {
        const ref = cell.getAttribute('r') ?? ''
        const r = rowIndex(ref)
        const c = colIndex(ref)
        if (r >= MAX_ROWS || c >= MAX_COLS) {
          truncated = true
          continue
        }

        const type = cell.getAttribute('t') ?? 'n'
        let value = ''
        if (type === 'inlineStr') {
          value = textOf(cell, 't')
        } else {
          const v = cell.getElementsByTagName('v')[0]
          value = v?.textContent ?? ''
          if (type === 's') {
            value = shared[Number(value)] ?? ''
          }
        }
        if (!value) continue

        while (rows.length <= r) rows.push([])
        rows[r]![c] = value
      }
    }

    const displayRows = rows.slice(0, MAX_ROWS).map((row) => {
      const filled: string[] = []
      for (let c = 0; c < MAX_COLS; c++) filled.push(row[c] ?? '')
      return filled
    })
    if (rows.length > MAX_ROWS) truncated = true

    return { name: names[i] ?? `Sheet${i + 1}`, rows: displayRows, truncated }
  })
}

function previewPptx(entries: Record<string, Uint8Array>, notes: string[]): SlidePreview[] {
  const files = Object.keys(entries)
    .filter((k) => /^ppt\/slides\/slide\d+\.xml$/.test(k))
    .sort((a, b) => {
      const na = Number(a.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
      const nb = Number(b.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
      return na - nb
    })

  if (files.length === 0) {
    notes.push('未找到幻灯片 XML，可能不是标准 pptx')
    return []
  }

  return files.map((path, i) => {
    const xml = readEntry(entries, path)
    const texts = xml
      ? Array.from(parseXml(xml).getElementsByTagName('a:t'))
          .map((t) => (t.textContent ?? '').trim())
          .filter(Boolean)
          .slice(0, MAX_SLIDE_TEXTS)
      : []
    return { index: i + 1, texts }
  })
}

export function inspectOoxml(kind: DocKind, bytes: Uint8Array): PreviewModel {
  let entries: Record<string, Uint8Array>
  try {
    entries = unzipSync(bytes)
  } catch {
    throw new Error('文件不是有效的 OOXML 压缩包（可能已损坏，或扩展名与实际格式不符）')
  }

  const notes: string[] = []
  const base = { kind, entryCount: Object.keys(entries).length, notes }

  if (kind === 'docx') return { ...base, paragraphs: previewDocx(entries, notes) }
  if (kind === 'xlsx') return { ...base, sheets: previewXlsx(entries, notes) }
  return { ...base, slides: previewPptx(entries, notes) }
}
