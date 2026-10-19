<script setup lang="ts">
import { defineAsyncComponent, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { VueDraggable } from 'vue-draggable-plus'
import { CircleAlert, Plus, RefreshCw } from '@lucide/vue'
import { Button } from '@/components/ui/button'
import CardActionsButton from '@/components/common/CardActionsButton.vue'
import ActionMenu from '@/components/common/ActionMenu.vue'
import { confirmDialog, toast } from '@/lib/feedback'
import AppIconSystemMonitor from './AppIconSystemMonitor/index.vue'
import type { MonitorData } from './typings'
import { deleteByIndex, getAll, saveAll } from './common'
import { persistMonitorOrder } from './order'
import { PanelPanelConfigStyleEnum } from '@/enums'
import { equalJson } from '@/utils/confirmedWrite'

const props = defineProps<{
  allowEdit?: boolean
  showTitle?: boolean
}>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const Edit = defineAsyncComponent(() => import('./Edit/index.vue'))

const dropdownMenuX = ref(0)
const dropdownMenuY = ref(0)
const dropdownShow = ref(false)
const currentRightSelectIndex = ref<number | null>(null)

const sortSaving = ref(false)
const dragging = ref(false)
const needsReload = ref(false)
const reloading = ref(false)
watch(sortSaving, value => emit('busy', value))
onUnmounted(() => emit('busy', false))
let beforeDrag: MonitorData[] = []
let suppressClickUntil = 0
const cardKeys = new WeakMap<object, number>()
let nextCardKey = 0
function cardKey(item: MonitorData) {
  if (!cardKeys.has(item))
    cardKeys.set(item, nextCardKey++)
  return cardKeys.get(item)!
}

const editShowStatus = ref<boolean>(false)
const editorMounted = ref(false)
watch(editShowStatus, (visible) => {
  if (visible)
    editorMounted.value = true
})
const editData = ref<MonitorData | null>(null)
const editIndex = ref<number | null>(null)

function handleAddItem() {
  if (!props.allowEdit || sortSaving.value || dragging.value || needsReload.value)
    return
  editShowStatus.value = true
  editData.value = null
  editIndex.value = null
}

const monitorDatas = ref<MonitorData[]>([])

function monitorStyle(item: MonitorData): PanelPanelConfigStyleEnum {
  return item.extendParam?.style === PanelPanelConfigStyleEnum.info
    ? PanelPanelConfigStyleEnum.info
    : PanelPanelConfigStyleEnum.icon
}

function handleClick(index: number, item: MonitorData) {
  if (!props.allowEdit || sortSaving.value || dragging.value || needsReload.value || Date.now() < suppressClickUntil)
    return
  editShowStatus.value = true
  editData.value = item
  editIndex.value = index
}

async function getData() {
  if (reloading.value || sortSaving.value)
    return
  reloading.value = true
  try {
    monitorDatas.value = await getAll()
    needsReload.value = false
  }
  catch {
    needsReload.value = true
    toast.error('操作失败')
  }
  finally {
    reloading.value = false
  }
}

watch(() => props.allowEdit, (enabled) => {
  if (!enabled) {
    dropdownShow.value = false
    editShowStatus.value = false
  }
})

onMounted(() => {
  getData()
})

function handleSaveDone() {
  getData()
}

function handleDragStart() {
  beforeDrag = JSON.parse(JSON.stringify(monitorDatas.value))
  dragging.value = true
  dropdownShow.value = false
}

async function handleDragEnd() {
  // Sortable dispatches end before Vue finishes applying its new list.
  await nextTick()
  dragging.value = false
  suppressClickUntil = Date.now() + 250
  sortSaving.value = true
  try {
    const result = await persistMonitorOrder(beforeDrag, monitorDatas.value, { save: saveAll, read: getAll })
    if (!result.confirmed || !equalJson(monitorDatas.value, result.value))
      monitorDatas.value = result.value
    needsReload.value = result.needsReload
    if (!result.confirmed)
      toast.error(result.error instanceof Error ? result.error.message : '保存失败')
  }
  finally {
    sortSaving.value = false
  }
}

function handleContextMenu(e: MouseEvent, index: number) {
  if (!props.allowEdit || sortSaving.value || dragging.value || needsReload.value || reloading.value || Date.now() < suppressClickUntil)
    return
  e.preventDefault()
  currentRightSelectIndex.value = index
  const rect = (e.currentTarget as HTMLElement)?.getBoundingClientRect()
  dropdownMenuX.value = e.clientX || rect?.left || 0
  dropdownMenuY.value = e.clientY || rect?.bottom || 0
  dropdownShow.value = true
}

function getDropdownMenuOptions() {
  const dropdownMenuOptions = [
    { label: '编辑', key: 'edit' },
    {
      label: '删除',
      key: 'delete',
    },
  ]

  return dropdownMenuOptions
}

async function deleteOneByIndex(index: number) {
  const res = await deleteByIndex(index)
  if (res)
    await getData()
  else
    toast.error('保存失败')
}

async function handleRightMenuSelect(key: string | number) {
  dropdownShow.value = false

  if (!props.allowEdit || sortSaving.value || dragging.value || needsReload.value)
    return
  switch (key) {
    case 'edit':
      if (currentRightSelectIndex.value !== null) {
        const item = monitorDatas.value[currentRightSelectIndex.value]
        if (item)
          handleClick(currentRightSelectIndex.value, item)
      }
      break
    case 'delete': {
      if (monitorDatas.value.length <= 1) {
        toast.warning('请至少保留一项')
        return
      }
      const index = currentRightSelectIndex.value
      if (index !== null && await confirmDialog({
        title: '警告',
        description: '你确定要删除吗？',
        confirmText: '确定',
        cancelText: '取消',
        destructive: true,
      })) {
        if (props.allowEdit)
          await deleteOneByIndex(index)
      }
      break
    }
    default:
      break
  }
}
</script>

<template>
  <div class="system-monitor w-full">
    <section class="mt-[50px]" aria-label="系统状态">
      <div class="system-monitor-header relative mb-5 ml-2.5 flex h-9 items-center gap-2 text-xl font-extrabold text-white">
        <h2 v-if="showTitle" class="text-shadow">
          系统状态
        </h2>
        <Button v-if="allowEdit" type="button" size="icon-sm" class="panel-button" :disabled="sortSaving || dragging || needsReload || reloading" :aria-label="`${'添加'} ${'系统状态'}`" @click="handleAddItem">
          <Plus class="size-4" />
        </Button>
        <span v-if="sortSaving" class="flex items-center gap-1 text-xs font-normal" role="status" aria-live="polite">
          <RefreshCw class="size-3 animate-spin" aria-hidden="true" />
          保存
        </span>
        <Button v-else-if="needsReload" type="button" variant="ghost" size="sm" class="gap-1 text-xs text-white hover:bg-black/30 hover:text-white" :disabled="reloading" @click="getData">
          <CircleAlert class="size-3" />
          重试
        </Button>
      </div>

      <VueDraggable
        v-model="monitorDatas"
        :animation="200"
        class="monitor-grid"
        :group="{ name: 'system-monitors', pull: false, put: false }"
        draggable=".monitor-card"
        filter="button, a, input, [data-no-drag]"
        :prevent-on-filter="false"
        :disabled="!allowEdit || sortSaving || needsReload || reloading || editShowStatus"
        ghost-class="monitor-drag-ghost"
        chosen-class="monitor-drag-chosen"
        :delay="180"
        :delay-on-touch-only="true"
        :touch-start-threshold="5"
        :fallback-tolerance="5"
        :force-fallback="true"
        :fallback-on-body="true"
        @start="handleDragStart"
        @end="handleDragEnd"
      >
        <div
          v-for="(item, index) in monitorDatas"
          :key="cardKey(item)"
          :title="item.description"
          class="monitor-card relative min-w-0"
          :class="{ 'monitor-card-info': monitorStyle(item) === PanelPanelConfigStyleEnum.info, 'cursor-grab active:cursor-grabbing': allowEdit && !sortSaving && !needsReload }"
          @click="handleClick(index, item)"
          @contextmenu="handleContextMenu($event, index)"
        >
          <CardActionsButton v-if="allowEdit" :label="`${'更多操作'}: ${item.monitorType.toUpperCase()}`" :disabled="sortSaving || dragging || needsReload || reloading" @click="handleContextMenu($event, index)" />
          <AppIconSystemMonitor
            :extend-param="item.extendParam"
            :card-type-style="monitorStyle(item)"
            :monitor-type="item.monitorType"
            :icon-text-color="item.extendParam?.color"
          />
        </div>
      </VueDraggable>
    </section>

    <Edit v-if="editorMounted" v-model:visible="editShowStatus" :monitor-data="editData" :index="editIndex" @done="handleSaveDone" />
    <ActionMenu v-model:open="dropdownShow" :x="dropdownMenuX" :y="dropdownMenuY" :options="getDropdownMenuOptions()" @select="handleRightMenuSelect" />
  </div>
</template>

<style scoped>
.text-shadow {
  text-shadow: 2px 2px 50px rgb(0, 0, 0);
}

.system-monitor {
  container-type: inline-size;
}

.monitor-grid {
  width: 100%;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(85px, 100%), 1fr));
  align-items: start;
  gap: 18px;
}

.monitor-card-info {
  grid-column: span 2;
}

.monitor-card :deep(.item-card-small-title) {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

@container (width < 188px) {
  .monitor-card-info {
    grid-column: span 1;
  }
}

.monitor-drag-ghost {
  opacity: 0.3;
}

.monitor-drag-chosen {
  outline: 1px dashed #ffffffa0;
  outline-offset: 5px;
  border-radius: 1rem;
}

</style>
