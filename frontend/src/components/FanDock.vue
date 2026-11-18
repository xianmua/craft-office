<template>
  <!-- 贴边竖条（全局唯一入口）：可上下拖拽，点击展开面板；悬停向左扇形展开楔形菜单 -->
  <div
    class="fixed right-0 z-30"
    :style="{ top: barTop + 'px' }"
    @mouseenter="menuEnter"
    @mouseleave="menuLeave"
  >
    <!-- 扇形菜单：竖条中心为圆心，4 个楔形扇区平分 180° 半圆辐射展开 -->
    <svg
      class="absolute pointer-events-none"
      :style="{ left: '-112px', top: '-72px', width: '240px', height: '240px' }"
      viewBox="0 0 240 240"
    >
      <g
        v-for="(m, i) in FAN_ITEMS"
        :key="m.key"
        class="fan-sector"
        :class="{ open: menuOpen }"
        :style="{ transitionDelay: (menuOpen ? i * 30 : 0) + 'ms' }"
      >
        <path
          class="fan-wedge"
          :d="wedgePath(m.mid)"
          :fill="m.color"
          :stroke="fanActive(m) ? '#ffffff' : 'none'"
          :stroke-width="fanActive(m) ? 2.5 : 0"
          @click="onFan(m)"
        />
        <text class="fan-label" :x="fanText(m.mid).x" :y="fanText(m.mid).y">{{ m.label }}</text>
      </g>
    </svg>

    <!-- 竖条本体 -->
    <div
      class="flex flex-col items-center justify-center gap-1 w-4 h-24 bg-amber-400 border border-r-0 border-amber-500 rounded-l-lg cursor-pointer select-none touch-none"
      @pointerdown="onBarDown"
      @click="onBarClick"
    >
      <span v-if="busy" class="w-1 h-1 rounded-full bg-white animate-pulse"></span>
      <svg class="w-3 h-3 text-white" viewBox="0 0 16 16" fill="none">
        <path d="M10 3L5 8l5 5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { aiEngine } from '@/artcraft/aiEngine'

const props = defineProps<{ busy?: boolean; engines?: Array<{ id: string; label: string }> }>()
const emit = defineEmits<{ (e: 'action', key: string): void; (e: 'expand'): void }>()
onBeforeUnmount(() => window.clearTimeout(closeTimer))

// ---- 楔形扇形菜单（信息图样式）：SVG viewBox 240×240，圆心 (120,120) ----
// 4 个扇区平分 180° 半圆（每扇 45°，留 2° 缝），从正上辐射到正下；四角圆弧倒角
const menuOpen = ref(false)
let closeTimer: number | undefined

/** 移出延迟收起（600ms），期间移回自动取消，避免误扫过就消失 */
function menuEnter() {
  window.clearTimeout(closeTimer)
  menuOpen.value = true
}
function menuLeave() {
  window.clearTimeout(closeTimer)
  closeTimer = window.setTimeout(() => (menuOpen.value = false), 600)
}
const FAN_CX = 120
const FAN_CY = 120
const FAN_R0 = 40
const FAN_R1 = 112
const RC_OUT = 10 // 外角圆角半径
const RC_IN = 6 // 内角圆角半径

interface FanItem {
  key: string // 引擎扇区 = 引擎 id；功能扇区 = save/settings
  label: string
  mid: number // 扇区中心角（度）
  color: string
  engineId?: string
}

/** 菜单上限 5 块：3 引擎 + 文件保存 + 设置；引擎由注册表（manifest.json engine 节）驱动，不放即不出现 */
const MAX_ENGINES = 3
const engineCount = computed(() => Math.min(props.engines?.length ?? 0, MAX_ENGINES))

/** 扇区动态生成：AI 引擎（注册表驱动，新引擎自动出现）+ 文件保存 + 设置，平分 180° 半圆 */
const FAN_ITEMS = computed<FanItem[]>(() => {
  const engines = (props.engines ?? []).slice(0, MAX_ENGINES)
  const n = engines.length + 2
  const step = 180 / n
  const colors = ['#F59E0B', '#FBBF24', '#FCD34D', '#FDE68A', '#FEF3C7']
  const items: FanItem[] = engines.map((e, i) => ({
    key: e.id,
    label: e.label,
    engineId: e.id,
    mid: 90 + step / 2 + i * step,
    color: colors[i % colors.length],
  }))
  items.push(
    { key: 'save', label: '文件保存', mid: 90 + step / 2 + engines.length * step, color: colors[engines.length % colors.length] },
    { key: 'settings', label: '设置', mid: 90 + step / 2 + (engines.length + 1) * step, color: colors[(engines.length + 1) % colors.length] },
  )
  return items
})
/** 扇区半宽（度）：随扇区数缩放，留 2° 缝隙 */
function fanHalf(): number {
  const n = engineCount.value + 2
  return 180 / n / 2 - 1
}

/** 圆角楔形扇环 path：径向直边 + 内外弧，四个角用二次贝塞尔倒圆 */
function wedgePath(mid: number): string {
  const half = fanHalf()
  const rad = (d: number) => (d * Math.PI) / 180
  const a0 = rad(mid - half)
  const a1 = rad(mid + half)
  const thOut = RC_OUT / FAN_R1 // 外弧上倒角对应的角度
  const thIn = RC_IN / FAN_R0
  const p = (r: number, a: number) =>
    `${(FAN_CX + r * Math.cos(a)).toFixed(2)},${(FAN_CY - r * Math.sin(a)).toFixed(2)}`
  return [
    `M ${p(FAN_R0, a0 + thIn)}`,
    `L ${p(FAN_R0 + RC_IN, a0)}`,
    `L ${p(FAN_R1 - RC_OUT, a0)}`,
    `Q ${p(FAN_R1, a0)} ${p(FAN_R1, a0 + thOut)}`,
    `A ${FAN_R1} ${FAN_R1} 0 0 1 ${p(FAN_R1, a1 - thOut)}`,
    `Q ${p(FAN_R1, a1)} ${p(FAN_R1 - RC_OUT, a1)}`,
    `L ${p(FAN_R0 + RC_IN, a1)}`,
    `Q ${p(FAN_R0, a1)} ${p(FAN_R0, a1 - thIn)}`,
    `A ${FAN_R0} ${FAN_R0} 0 0 0 ${p(FAN_R0, a0 + thIn)}`,
    `Q ${p(FAN_R0, a0)} ${p(FAN_R0 + RC_IN, a0)}`,
    'Z',
  ].join(' ')
}

/** 楔形中径处文字坐标（水平排布，易读优先） */
function fanText(mid: number): { x: number; y: number } {
  const r = (FAN_R0 + FAN_R1) / 2
  const a = (mid * Math.PI) / 180
  return { x: +(FAN_CX + r * Math.cos(a)).toFixed(1), y: +(FAN_CY - r * Math.sin(a)).toFixed(1) }
}

/** 当前引擎所在扇区画白圈标识 */
function fanActive(m: FanItem): boolean {
  return !!m.engineId && m.engineId === aiEngine.value
}

function onFan(m: FanItem) {
  menuOpen.value = false
  emit('action', m.key)
}

// ---- 竖条：按住上下拖拽，点击（未拖动时）展开 ----
const barTop = ref(Math.round(window.innerHeight / 3))
let dragStartY = 0
let dragged = false

function onBarDown(e: PointerEvent) {
  menuOpen.value = false // 拖拽中不弹扇形菜单
  const el = e.currentTarget as HTMLElement
  // 锁定指针：拖进内核 iframe 松开也能收到 up，避免监听残留导致"不按住也拖"
  let target: HTMLElement | Window = window
  try {
    el.setPointerCapture(e.pointerId)
    target = el
  } catch { /* keep window */ }
  dragStartY = e.clientY - barTop.value
  dragged = false
  const move = (ev: Event) => {
    const p = ev as PointerEvent
    const top = p.clientY - dragStartY
    if (Math.abs(top - barTop.value) > 3) dragged = true
    barTop.value = Math.min(window.innerHeight - 110, Math.max(0, top))
  }
  const up = () => {
    target.removeEventListener('pointermove', move)
    target.removeEventListener('pointerup', up)
    target.removeEventListener('pointercancel', up)
    window.removeEventListener('blur', up)
  }
  target.addEventListener('pointermove', move)
  target.addEventListener('pointerup', up)
  target.addEventListener('pointercancel', up)
  window.addEventListener('blur', up)
}

function onBarClick() {
  if (!dragged) emit('expand')
}
</script>

<style scoped>
/* 扇形楔形菜单：围绕圆心缩放弹出，扇区整体 hover 变色加粗 */
.fan-sector {
  opacity: 0;
  transform: scale(0.55);
  transform-box: view-box;
  transform-origin: 50% 50%;
  transition:
    transform 0.18s ease-out,
    opacity 0.15s ease-out;
  pointer-events: none;
}
.fan-sector.open {
  opacity: 1;
  transform: scale(1);
  pointer-events: auto;
  cursor: pointer;
}
.fan-wedge {
  transition: filter 0.12s;
}
.fan-sector:hover .fan-wedge {
  filter: brightness(0.85) saturate(1.15);
}
.fan-label {
  font-size: 12px;
  fill: #92400e;
  text-anchor: middle;
  dominant-baseline: middle;
  pointer-events: none;
  transition: fill 0.12s;
}
.fan-sector:hover .fan-label {
  fill: #ffffff;
  font-weight: 700;
}
</style>
