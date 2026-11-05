<template>
  <t-dialog
    v-model:visible="show"
    header="插件市场"
    width="560px"
    :footer="false"
    destroy-on-close
  >
    <t-loading :loading="loading" text="加载中…">
      <div class="flex flex-col gap-3 max-h-[60vh] overflow-y-auto pr-1">
        <p v-if="error" class="text-sm text-red-500 m-0">{{ error }}</p>
        <p v-else-if="entries.length === 0" class="text-sm text-muted m-0">市场为空（backend/plugins/market.json）。</p>
        <div
          v-for="entry in entries"
          :key="entry.id"
          class="border border-gray-200 rounded-lg px-4 py-3 flex flex-col gap-1"
        >
          <div class="flex items-center gap-2">
            <span class="font-medium text-sm">{{ entry.name }}</span>
            <t-tag size="small" variant="light">v{{ entry.version }}</t-tag>
            <t-tag v-if="isInstalled(entry.id)" size="small" theme="success" variant="light">已安装</t-tag>
            <div class="flex-1" />
            <t-button
              v-if="!isInstalled(entry.id)"
              size="small"
              theme="primary"
              variant="outline"
              :loading="busyId === entry.id"
              @click="doInstall(entry.id)"
            >
              安装
            </t-button>
            <t-popconfirm
              v-else
              :content="`卸载 ${entry.name}？其命令将从悬浮球菜单移除。`"
              @confirm="doUninstall(entry.id)"
            >
              <t-button size="small" variant="outline" theme="danger" :loading="busyId === entry.id">卸载</t-button>
            </t-popconfirm>
          </div>
          <p class="text-sm text-muted leading-5 m-0">{{ entry.description }}</p>
        </div>
      </div>
    </t-loading>
  </t-dialog>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { MessagePlugin } from 'tdesign-vue-next'
import { fetchPlugins, installPlugin, uninstallPlugin, type PluginCatalog } from '@/api/plugins'

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ (e: 'update:visible', v: boolean): void; (e: 'changed'): void }>()

const show = computed({
  get: () => props.visible,
  set: (v: boolean) => emit('update:visible', v),
})

const loading = ref(false)
const busyId = ref('')
const error = ref('')
const catalog = ref<PluginCatalog>({ market: [], installed: [] })

const entries = computed(() => catalog.value.market)

function isInstalled(id: string): boolean {
  return catalog.value.installed.includes(id)
}

async function refresh() {
  loading.value = true
  error.value = ''
  try {
    catalog.value = await fetchPlugins()
  } catch (err) {
    error.value = err instanceof Error ? err.message : '插件目录加载失败'
  } finally {
    loading.value = false
  }
}

async function doInstall(id: string) {
  busyId.value = id
  try {
    await installPlugin(id)
    await refresh()
    emit('changed')
    MessagePlugin.success('插件已安装')
  } catch (err) {
    MessagePlugin.error(err instanceof Error ? err.message : '安装失败')
  } finally {
    busyId.value = ''
  }
}

async function doUninstall(id: string) {
  busyId.value = id
  try {
    await uninstallPlugin(id)
    await refresh()
    emit('changed')
    MessagePlugin.success('插件已卸载')
  } catch (err) {
    MessagePlugin.error(err instanceof Error ? err.message : '卸载失败')
  } finally {
    busyId.value = ''
  }
}

defineExpose({ refresh })
</script>
