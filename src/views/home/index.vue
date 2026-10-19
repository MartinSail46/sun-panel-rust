<script setup lang="ts">
import { VueDraggable } from 'vue-draggable-plus'
import { Button } from '@/components/ui/button'
import ActionMenu from '@/components/common/ActionMenu.vue'
import RoundCardModal from '@/components/common/RoundCardModal/index.vue'
import { Spinner } from '@/components/ui/spinner'
import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Info, Undo2 } from '@lucide/vue'
import CardActionsButton from '@/components/common/CardActionsButton.vue'
import type { SortableEvent } from 'sortablejs'
import { toast, confirmDialog } from '@/lib/feedback'
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { AppIcon, AppStarter, EditItem } from './components'
import { Clock, SearchBox, SystemMonitor } from '@/components/deskModule'
import { SvgIcon } from '@/components/common'
import { setTitle } from '@/utils/cmn'
import { usePanelState } from '@/store'
import { useNavigationStore } from '@/store/modules/navigation'
import { cloneDraft, filterNavigation, selectionIndex } from '@/store/modules/navigation/helpers'
import { PanelPanelConfigStyleEnum } from '@/enums'
import { DEFAULT_WALLPAPER, normalizeWallpaperSource } from '@/utils/wallpaper'

const ms = toast
const panelState = usePanelState()
const wallpaperFailed = ref(false)
const wallpaperSource = computed(() => normalizeWallpaperSource(panelState.panelConfig.backgroundImageSrc))
const wallpaperSrc = computed(() => wallpaperFailed.value ? DEFAULT_WALLPAPER : wallpaperSource.value)
watch(wallpaperSource, () => { wallpaperFailed.value = false })
function handleWallpaperError() {
  if (wallpaperSrc.value !== DEFAULT_WALLPAPER) wallpaperFailed.value = true
}
const navigation = useNavigationStore()
function isDetailIcon(item: Panel.ItemInfo) {
  return item.icon?.style === PanelPanelConfigStyleEnum.info
}
const historyText = { undo: '撤销', move: '撤销移动', sort: '撤销分组排序', delete: '撤销删除', undone: '已撤销', trash: '已移入回收站，可在系统应用中恢复', outdated: '已有后续操作，请使用工具栏撤销最近的操作', conflict: '导航已发生变化，已加载最新内容' }
const undoLabel = computed(() => historyText[navigation.undoRecord?.action.kind === 'delete' ? 'delete' : navigation.undoRecord?.action.kind === 'sort' ? 'sort' : 'move'])
const editing = ref(false)
const scrollContainerRef = ref<HTMLElement>()
const scrolled = ref(false)
function handleScroll() { scrolled.value = (scrollContainerRef.value?.scrollTop ?? 0) > 150 }
const editItemInfoShow = ref(false)
const editItemInfoData = ref<Panel.ItemInfo | null>(null)
const currentAddItemGroupId = ref<number>()
const settingModalShow = ref(false)
const editItemMounted = ref(false)
const appStarterMounted = ref(false)
const searchKeyword = ref('')
const monitorSaving = ref(false)
const dragging = ref(false)
const moveSaved = ref(false)
let dragBase: ReturnType<typeof navigation.snapshot> | undefined
let suppressClickUntil = 0
const dragGroup = { name: 'navigation-items', pull: true, put: true }
const navigationBusy = computed(() => navigation.saving || navigation.needsVerification)
const canDrag = computed(() => editing.value && !navigationBusy.value && !searchKeyword.value)
watch(editing, () => { moveSaved.value = false })
const selectedIndex = ref(0)
const filterItems = computed(() => filterNavigation(navigation.groups, searchKeyword.value))
const searchResults = computed(() => filterItems.value.flatMap(group => group.items))
const selectedItem = computed(() => searchKeyword.value ? searchResults.value[selectedIndex.value] : undefined)
const text = { editMode: '编辑面板', done: '完成编辑', editHint: '直接拖动应用到任意分组和位置，松手自动保存。点击卡片编辑，搜索时暂停拖动。', help: '编辑帮助', saving: '保存中…', saved: '已保存', autoSave: '拖动自动保存', verify: '核对结果', unverified: '结果待核对', latest: '已载入最新排列', searchPaused: '清空搜索后拖动', dropHere: '拖到此分组', noItems: '暂无应用', empty: '没有匹配的应用', noGroups: '暂无分组，请在系统应用中添加分组。', retry: '重试', copy: '复制地址', copied: '地址已复制', copyFail: '无法自动复制，请选中地址手动复制', embeddedHint: '部分网站不允许嵌入，空白或无法访问时请在新标签打开。', waiting: '等待页面响应…', timedOut: '页面响应较慢，或目标网站拒绝嵌入。', address: '地址', preview: '小窗口', settings: '系统应用', loading: '正在加载应用…' }
watch(editItemInfoShow, visible => { if (visible) editItemMounted.value = true })
watch(settingModalShow, visible => { if (visible) appStarterMounted.value = true })
watch(searchKeyword, () => selectedIndex.value = 0)
watch(() => searchResults.value.length, length => selectedIndex.value = Math.max(0, Math.min(selectedIndex.value, length - 1)))
watch(() => panelState.panelConfig.logoText, title => { if (title) setTitle(title) }, { immediate: true })

const dropdownX = ref(0)
const dropdownY = ref(0)
const dropdownShow = ref(false)
const menuItem = ref<Panel.ItemInfo | null>(null)
const menuOptions = computed(() => [
  { label: '新窗口打开', key: 'newTab' },
  { label: text.copy, key: 'copy' },
  { label: '编辑', key: 'edit' },
  ...(editing.value ? [{ label: '删除', key: 'delete' }] : []),
])
function showMenu(event: MouseEvent, item: Panel.ItemInfo) {
  if (!editing.value || navigationBusy.value || dragging.value || Date.now() < suppressClickUntil)
    return
  event.preventDefault()
  menuItem.value = item
  const rect = (event.currentTarget as HTMLElement)?.getBoundingClientRect()
  dropdownX.value = event.clientX || rect?.left || 0
  dropdownY.value = event.clientY || rect?.bottom || 0
  dropdownShow.value = true
}
async function copyAddress(url: string) {
  try {
    await navigator.clipboard.writeText(url)
    ms.success(text.copied)
  }
  catch {
    ms.warning(text.copyFail)
  }
}
async function handleMenu(key: string | number) {
  dropdownShow.value = false
  const item = menuItem.value
  if (!item)
    return
  if (key === 'newTab')
    window.open(item.url, '_blank', 'noopener,noreferrer')
  else if (key === 'copy')
    void copyAddress(item.url)
  else if (key === 'edit') {
    editing.value = true
    handleEditItem(item)
  }
  else if (key === 'delete') {
    if (navigation.busy) return
    if (!await confirmDialog({ title: '警告', description: `你确定要删除${item.title}吗？删除后可从回收站恢复。`, confirmText: '确定', cancelText: '取消' })) return
    try {
      await navigation.deleteItem(item.id!)
      const record = navigation.undoRecord
      ms.success(historyText.trash, { action: { label: historyText.undo, onClick: () => { if (navigation.undoRecord === record) void undoNavigation(); else ms.info(historyText.outdated) } } })
    }
    catch (error) { ms.error(error instanceof Error ? error.message : '删除失败') }

  }
}
async function undoNavigation() {
  if (!navigation.canUndo) return
  try {
    await navigation.undo()
    moveSaved.value = false
    ms.success(historyText.undone)
  }
  catch (cause) {
    if (navigation.needsVerification) ms.warning('操作结果尚未确认，请先刷新检查，避免重复提交。')
    else if (cause && typeof cause === 'object' && 'code' in cause && cause.code === 1409) ms.warning(historyText.conflict)
    else ms.error(cause instanceof Error ? cause.message : '保存失败')
  }
}
function handleUndoKey(event: KeyboardEvent) {
  if (!editing.value || !navigation.canUndo || event.defaultPrevented || event.isComposing || !(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey || event.key.toLowerCase() !== 'z') return
  if (document.querySelector('[role="dialog"], [role="alertdialog"]') || (event.target instanceof Element && event.target.closest('input, textarea, [contenteditable]:not([contenteditable="false"])'))) return
  event.preventDefault()
  void undoNavigation()
}
onMounted(() => window.addEventListener('keydown', handleUndoKey))
onUnmounted(() => window.removeEventListener('keydown', handleUndoKey))
function handleEditItem(item: Panel.ItemInfo) {
  if (navigationBusy.value) return
  editItemInfoData.value = cloneDraft(item)
  currentAddItemGroupId.value = undefined
  editItemInfoShow.value = true
}
function handleAddItem(groupId: number | undefined) {
  if (navigationBusy.value) return
  editItemInfoData.value = null
  currentAddItemGroupId.value = groupId
  editItemInfoShow.value = true
}
function handleItemClick(event: MouseEvent, item: Panel.ItemInfo) {
  if (Date.now() < suppressClickUntil || dragging.value) { event.preventDefault(); return }
  if (editing.value) {
    event.preventDefault()
    handleEditItem(item)
  }
  else if (item.openMethod === 3 && !event.ctrlKey && !event.metaKey && !event.shiftKey && !event.altKey && event.button === 0) {
    event.preventDefault()
    openPreview(item)
  }
}
function activateSearch() {
  const item = selectedItem.value
  if (!item)
    return
  if (editing.value)
    handleEditItem(item)
  else if (item.openMethod === 3)
    openPreview(item)
  else if (item.openMethod === 1)
    window.location.assign(item.url)
  else
    window.open(item.url, '_blank', 'noopener,noreferrer')
}
function moveSelection(delta: number) {
  selectedIndex.value = selectionIndex(selectedIndex.value, delta, searchResults.value.length)
  document.querySelector(`[data-item-id="${selectedItem.value?.id}"]`)?.scrollIntoView({ block: 'nearest' })
}
function startDrag() {
  dragBase = navigation.snapshot()
  dragging.value = true
  moveSaved.value = false
  dropdownShow.value = false
}
async function endDrag(event: SortableEvent) {
  const snapshot = dragBase
  dragBase = undefined
  await nextTick()
  dragging.value = false
  suppressClickUntil = Date.now() + 300
  if (!snapshot) return
  const itemId = Number(event.item.dataset.itemId)
  const targetGroupId = Number(event.to.dataset.groupId)
  const targetIndex = event.newDraggableIndex ?? event.newIndex
  if (searchKeyword.value || targetIndex === undefined) {
    navigation.groups = snapshot.groups
    return
  }
  try {
    await navigation.move(itemId, targetGroupId, targetIndex, snapshot)
    moveSaved.value = true
  }
  catch (error) {
    if (error instanceof RangeError) navigation.groups = snapshot.groups
    if (navigation.needsVerification) ms.warning('操作结果尚未确认，请先刷新检查，避免重复提交。')
    else if (error && typeof error === 'object' && 'code' in error && error.code === 1409) ms.warning(text.latest)
    else ms.error(error instanceof Error ? error.message : '保存失败')
  }
}
async function verifyMove() {
  try {
    const result = await navigation.verifyMove()
    moveSaved.value = result === 'verified'
    ms.info(result === 'verified' ? text.saved : text.latest)
  }
  catch { ms.warning('操作结果尚未确认，请先刷新检查，避免重复提交。') }
}
function finishEditing() {
  if (navigation.saving || monitorSaving.value || dragging.value) return
  dropdownShow.value = false
  editing.value = false
}
async function refreshNavigation() {
  try { await navigation.refresh() }
  catch { ms.error('加载失败，请重试') }
}

const windowShow = ref(false)
const windowSrc = ref('')
const windowTitle = ref('')
const frameWaiting = ref(false)
const frameTimedOut = ref(false)
let frameTimer: ReturnType<typeof setTimeout> | undefined
function clearFrameTimer() {
  clearTimeout(frameTimer)
  frameTimer = undefined
}
function openPreview(item: Panel.ItemInfo) {
  clearFrameTimer()
  windowSrc.value = item.url
  windowTitle.value = item.title
  frameWaiting.value = true
  frameTimedOut.value = false
  windowShow.value = true
  frameTimer = setTimeout(() => {
    frameWaiting.value = false
    frameTimedOut.value = true
  }, 8000)
}
function frameResponded() {
  clearFrameTimer()
  frameWaiting.value = false
}
watch(windowShow, visible => {
  if (!visible) {
    clearFrameTimer()
    windowSrc.value = ''
  }
})
onUnmounted(() => { clearFrameTimer(); navigation.cancel() })
onMounted(async () => {
  await Promise.allSettled([
    navigation.load(),
    panelState.loadPanelConfig(),
  ]).then(results => {
    if (results.some(result => result.status === 'rejected'))
      ms.error('加载失败，请重试')
  })
})
</script>

<template>
  <div class="w-full h-full sun-main">
    <img class="cover wallpaper" :src="wallpaperSrc" alt="" aria-hidden="true" decoding="async" :style="{ filter: `blur(${panelState.panelConfig.backgroundBlur}px)` }" @error="handleWallpaperError">
    <div class="mask" :style="{ backgroundColor: `rgba(0,0,0,${panelState.panelConfig.backgroundMaskNumber})` }" />
    <div ref="scrollContainerRef" class="panel-scroll absolute inset-0" @scroll="handleScroll">
      <main class="px-2.5 mx-auto" :style="{ paddingTop: `calc(10px + ${panelState.panelConfig.marginTop ?? 10}dvh)`, paddingBottom: `calc(32px + ${panelState.panelConfig.marginBottom ?? 10}dvh)`, maxWidth: (panelState.panelConfig.maxWidth ?? 1200) + panelState.panelConfig.maxWidthUnit }">
        <div class="mx-auto w-[94%] sm:w-[80%]">
          <div class="flex items-center justify-center text-white">
            <span class="text-2xl md:text-6xl font-bold text-shadow">{{ panelState.panelConfig.logoText }}</span>
            <span class="text-base lg:text-2xl mx-2.5">|</span>
            <div class="text-shadow">
              <Clock :hide-second="!panelState.panelConfig.clockShowSecond" />
            </div>
          </div>
          <div v-if="panelState.panelConfig.searchBoxShow" class="mt-5 mx-auto lg:w-[80%]">
            <SearchBox @item-search="searchKeyword = $event.trim()" @activate="activateSearch" @select="moveSelection" />
          </div>
        </div>
        <div :style="{ marginLeft: `${panelState.panelConfig.marginX}px`, marginRight: `${panelState.panelConfig.marginX}px` }">
          <SystemMonitor v-if="panelState.panelConfig.systemMonitorShow" :allow-edit="editing" :show-title="panelState.panelConfig.systemMonitorShowTitle" @busy="monitorSaving = $event" />
          <div v-if="navigation.error" role="alert" class="text-white bg-black/50 rounded-xl p-3 mt-5 flex items-center gap-3">
            <span>加载失败，请重试</span><Button size="sm" :disabled="navigation.loading" @click="refreshNavigation">
              {{ text.retry }}
            </Button>
          </div>
          <div v-if="navigation.loading" class="text-white mt-8 flex items-center gap-3" role="status">
            <span>{{ text.loading }}</span>
            <Button size="sm" @click="navigation.cancel()">
              取消加载
            </Button>
          </div>
          <div v-else-if="navigation.cancelled" class="text-white mt-8 flex items-center gap-3" role="status">
            <span>加载已取消</span>
            <Button size="sm" @click="refreshNavigation">
              {{ text.retry }}
            </Button>
          </div>
          <p v-else-if="searchKeyword && !filterItems.length" class="text-white mt-8 text-center" role="status">
            {{ text.empty }}
          </p>
          <p v-else-if="!navigation.groups.length && navigation.loaded" class="text-white mt-8 text-center">
            {{ text.noGroups }}
          </p>
          <section v-for="group in filterItems" :key="group.id" class="item-list mt-12" :aria-label="group.title">
            <div class="text-white mb-5 ml-2.5 flex h-9 items-center gap-2">
              <h2 class="text-xl font-extrabold text-shadow">
                {{ group.title }}
              </h2>
              <Button v-if="editing" type="button" variant="ghost" size="icon" class="panel-button size-8" :disabled="navigationBusy" title="添加" :aria-label="`添加 ${group.title}`" @click="handleAddItem(group.id)">
                <SvgIcon icon="typcn:plus" />
              </Button>
            </div>
            <div class="relative">
              <VueDraggable v-model="group.items" item-key="id" draggable=".app-card" :animation="180" :group="dragGroup" :data-group-id="group.id" class="app-grid" :class="{ 'all-detail-cards': group.items.length > 0 && group.items.every(isDetailIcon), 'drag-enabled': canDrag }" :disabled="!canDrag" filter="button,input,textarea,select" :prevent-on-filter="false" :delay-on-touch-only="true" :delay="160" :touch-start-threshold="4" :fallback-tolerance="4" :force-fallback="true" :fallback-on-body="true" :empty-insert-threshold="24" ghost-class="drag-ghost" chosen-class="drag-chosen" @start="startDrag" @end="endDrag">
                <div v-for="item in group.items" :key="item.id" class="app-card relative min-w-0" :data-item-id="item.id" :class="{ 'app-card-info': isDetailIcon(item), 'search-selected': selectedItem?.id === item.id }" @contextmenu="showMenu($event, item)">
                  <a :href="item.url" :draggable="!editing" :target="item.openMethod === 2 ? '_blank' : undefined" rel="noopener noreferrer" class="block app-link rounded-2xl" :title="item.description || item.title" :aria-label="editing ? `编辑 ${item.title}` : item.title" @click="handleItemClick($event, item)">
                    <AppIcon :item-info="item" />
                  </a>
                  <CardActionsButton v-if="editing" :disabled="navigationBusy" :label="`更多操作：${item.title}`" @click="showMenu($event, item)" />
                </div>
              </VueDraggable>
              <div v-if="!group.items.length" class="pointer-events-none absolute inset-0 flex items-center justify-center rounded-lg border border-dashed border-white/25 text-sm text-white/70">
                {{ canDrag ? text.dropHere : text.noItems }}
              </div>
            </div>
          </section>
        </div>
        <div v-if="panelState.panelConfig.footerHtml" class="mt-5 footer" v-html="panelState.panelConfig.footerHtml" />
      </main>
    </div>
    <ActionMenu :x="dropdownX" :y="dropdownY" :options="menuOptions" :open="editing && dropdownShow" @update:open="dropdownShow = $event" @select="handleMenu" />
    <div class="panel-toolbar" role="toolbar" :aria-label="text.settings">
      <div class="flex flex-wrap items-center justify-end gap-2">
        <Button v-if="navigation.undoCount" variant="ghost" class="panel-button" :disabled="!navigation.canUndo || monitorSaving || dragging" :title="`${undoLabel} · ⌘/Ctrl Z`" @click="undoNavigation">
          <Undo2 class="size-4" />{{ undoLabel }}
        </Button>
        <Badge v-if="editing || navigation.needsVerification" variant="outline" class="border-white/20 bg-black/70 text-white" role="status" aria-live="polite">
          <Spinner v-if="navigation.saving || monitorSaving" class="mr-1" />{{ navigation.saving || monitorSaving ? text.saving : navigation.needsVerification ? text.unverified : searchKeyword ? text.searchPaused : moveSaved ? text.saved : text.autoSave }}
        </Badge>
        <Button v-if="navigation.needsVerification" size="sm" class="panel-button" :disabled="navigation.saving" @click="verifyMove">
          {{ text.verify }}
        </Button>
        <Popover v-if="editing">
          <PopoverTrigger as-child>
            <Button variant="ghost" size="icon" class="panel-button" :aria-label="text.help">
              <Info class="size-4" />
            </Button>
          </PopoverTrigger>
          <PopoverContent align="end" class="w-72 text-sm">
            {{ text.editHint }}
          </PopoverContent>
        </Popover>
        <Button class="panel-button" :disabled="navigation.saving || monitorSaving || dragging" :aria-pressed="editing" @click="editing ? finishEditing() : editing = true">
          {{ editing ? text.done : text.editMode }}
        </Button>
        <Button class="panel-button" :disabled="navigationBusy || monitorSaving || dragging" :title="text.settings" :aria-label="text.settings" @click="settingModalShow = !settingModalShow">
          <SvgIcon icon="majesticons-applications" />
        </Button>
      </div>
    </div>
    <AppStarter v-if="appStarterMounted" v-model:visible="settingModalShow" />
    <Button v-if="scrolled" variant="secondary" size="icon" class="fixed bottom-3 right-3 z-10" aria-label="回到顶部" @click="scrollContainerRef?.scrollTo({ top: 0, behavior: 'smooth' })">
      ↑
    </Button>
    <EditItem v-if="editItemMounted" v-model:visible="editItemInfoShow" :item-info="editItemInfoData" :item-group-id="currentAddItemGroupId" />
    <RoundCardModal v-model:show="windowShow" :mask-closable="false" class="preview-modal max-w-[1100px]" style="height:min(800px,calc(100dvh - 24px))" :title="windowTitle || text.preview" :content-style="{ height: 'calc(100% - 48px)', display: 'flex', flexDirection: 'column', minHeight: '0' }">
      <div class="flex gap-2 items-center flex-wrap mb-2">
        <Input :model-value="windowSrc" readonly :aria-label="text.address" class="preview-address flex-1 min-w-0 p-2 rounded border" @focus="($event.target as HTMLInputElement).select()" />
        <Button size="sm" @click="copyAddress(windowSrc)">
          {{ text.copy }}
        </Button>
        <a :href="windowSrc" target="_blank" rel="noopener noreferrer" class="preview-open">新窗口打开</a>
      </div>
      <p class="text-xs mb-2 opacity-80">
        {{ text.embeddedHint }}
      </p>
      <p v-if="frameWaiting || frameTimedOut" role="status" class="flex items-center gap-2 text-sm mb-2">
        <Spinner v-if="frameWaiting" />{{ frameTimedOut ? text.timedOut : text.waiting }}
      </p>
      <iframe v-if="windowShow" :key="windowSrc" :src="windowSrc" :title="windowTitle || text.preview" class="w-full grow min-h-0 rounded-xl border" referrerpolicy="no-referrer" @load="frameResponded" />
    </RoundCardModal>
  </div>
</template>

<style>
body, html { overflow:hidden;background:rgb(54,54,54); }
</style>
<style scoped>
.mask,.cover { position:absolute;inset:0; }
.cover { width:100%;height:100%;object-fit:cover;pointer-events:none;transform:scale(1.05); }
.text-shadow { text-shadow:2px 2px 50px #000; }
.panel-scroll { overflow-x:hidden;overflow-y:auto;scrollbar-gutter:stable both-edges; }
.panel-toolbar { max-width:calc(100vw - 24px);position:fixed;right:max(12px,env(safe-area-inset-right));top:max(12px,env(safe-area-inset-top));z-index:5; }
.app-grid { display:grid;gap:18px;width:100%;min-height:94px;grid-template-columns:repeat(auto-fill,minmax(85px,1fr));container-type:inline-size; }
.app-card-info { grid-column:span 2; }
.all-detail-cards { min-height:70px;grid-template-columns:repeat(auto-fill,minmax(min(200px,100%),1fr)); }
.all-detail-cards .app-card-info { grid-column:auto; }
@container (max-width:187px) { .app-card-info { grid-column:auto; } }
.app-link { color:inherit;text-decoration:none; }
.app-link:focus-visible,.search-selected { outline:2px solid white;outline-offset:5px;border-radius:1rem; }
button:disabled { opacity:.4;cursor:default; }
button:focus-visible { outline:2px solid currentColor;outline-offset:2px; }
.drag-enabled .app-link { cursor:grab;user-select:none;-webkit-user-drag:none; }
.drag-chosen { cursor:grabbing; }
.drag-ghost { opacity:.35;outline:2px dashed #86efac;outline-offset:4px;border-radius:1rem; }
.preview-address { user-select:text;background:transparent;border-color:#8886; }
.preview-open { padding:5px 9px;border:1px solid #8886;border-radius:5px;white-space:nowrap; }
@media(max-width:500px) { .all-detail-cards { grid-template-columns:repeat(auto-fill,minmax(min(145px,100%),1fr)); } }
</style>
