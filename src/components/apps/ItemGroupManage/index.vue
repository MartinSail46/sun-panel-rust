<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { VueDraggable } from 'vue-draggable-plus'
import { edit } from '@/api/panel/itemIconGroup'
import { RoundCardModal, SvgIcon } from '@/components/common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { confirmDialog, toast } from '@/lib/feedback'
import { useNavigationStore } from '@/store/modules/navigation'
import { cloneDraft, moved } from '@/store/modules/navigation/helpers'
import { isRequestCancelled, isUncertainWrite } from '@/utils/requestLifetime'

const emit = defineEmits<{ busy: [value: boolean] }>()
const navigation = useNavigationStore()
const formRef = ref<HTMLFormElement | null>(null)
const modalShow = ref(false)
const editing = ref(false)
const saving = ref(false)
const deleting = ref<number | null>(null)
const sortSaving = ref(false)
const sortStatus = ref(false)
const titleError = ref('')
const groups = ref<Panel.ItemIconGroup[]>([])
const text = { up: '上移', down: '下移', cancelSort: '取消排序', undo: '撤销', undone: '已撤销', trashed: '已移入回收站，可恢复分组及本次删除的应用' }
const busy = computed(() => saving.value || deleting.value !== null || sortSaving.value || navigation.busy)
watch(busy, value => emit('busy', value), { immediate: true })
onUnmounted(() => emit('busy', false))
const emptyDraft = (): Panel.ItemIconGroup => ({ title: '', icon: 'material-symbols-ad-group-outline-rounded', sort: 9999 })
const draft = ref(emptyDraft())

watch(() => navigation.groups, (value) => {
  groups.value = value.map(({ items: _items, ...group }) => cloneDraft(group))
  sortStatus.value = false
}, { immediate: true })

function handleAddGroup() {
  if (busy.value) return
  editing.value = false
  draft.value = emptyDraft()
  titleError.value = ''
  modalShow.value = true
}
function handleEditGroup(group: Panel.ItemIconGroup) {
  if (busy.value) return
  editing.value = true
  draft.value = cloneDraft(group)
  titleError.value = ''
  modalShow.value = true
}
async function reportWriteError(error: unknown, fallback: string) {
  if (isUncertainWrite(error)) {
    await navigation.refresh().catch(() => {})
    toast.warning('操作结果尚未确认，请先刷新检查，避免重复提交。')
  }
  else if (!isRequestCancelled(error)) {
    toast.error(error instanceof Error ? error.message : fallback)
  }
}
async function handleSaveGroup() {
  if (busy.value)
    return
  titleError.value = draft.value.title?.trim() ? '' : '必填项'
  if (titleError.value) {
    await nextTick()
    formRef.value?.querySelector<HTMLInputElement>('input')?.focus()
    return
  }
  saving.value = true
  try {
    const response = await navigation.withMutation(() => edit(cloneDraft(draft.value)))
    if (response.code !== 0)
      throw new Error(response.msg || '保存失败')
    modalShow.value = false
    draft.value = emptyDraft()
    toast.success('保存成功')
    // Saving has been acknowledged; a later read must not report it as failed.
    await navigation.refresh().catch(() => {})
  }
  catch (error) {
    await reportWriteError(error, '保存失败')
  }
  finally {
    saving.value = false
  }
}
async function handleSaveSort() {
  if (busy.value)
    return
  sortSaving.value = true
  try {
    await navigation.sortGroups(groups.value.map((group, index) => ({ id: group.id!, sort: index + 1 })))
    sortStatus.value = false
    toast.success('保存成功')
  }
  catch (error) {
    const pendingOrder = cloneDraft(groups.value)
    await reportWriteError(error, '保存失败')
    if (isUncertainWrite(error)) {
      groups.value = pendingOrder
      sortStatus.value = true
    }
  }
  finally {
    sortSaving.value = false
  }
}
function cancelSort() {
  groups.value = navigation.groups.map(({ items: _items, ...group }) => cloneDraft(group))
  sortStatus.value = false
}
async function handleDelete(group: Panel.ItemIconGroup) {
  if (!group.id || busy.value)
    return
  deleting.value = group.id
  try {
    const confirmed = await confirmDialog({
      title: '警告',
      description: `确定将分组“${group.title}”及其中的应用移入回收站吗？删除后可从回收站恢复。`,
      confirmText: '确定',
      cancelText: '取消',
      destructive: true,
    })
    if (!confirmed)
      return
    await navigation.deleteGroup(group.id)
    toast.success(text.trashed)
  }
  catch (error) {
    await reportWriteError(error, '删除失败')
  }
  finally {
    deleting.value = null
  }
}
async function undoChange() {
  if (busy.value || !navigation.canUndo) return
  try { await navigation.undo(); toast.success(text.undone) }
  catch (cause) { await reportWriteError(cause, '保存失败') }
}
onMounted(() => navigation.load().catch(() => toast.error('加载失败，请重试')))
</script>

<template>
  <div class="flex h-full min-h-0 flex-col text-card-foreground">
    <div class="flex flex-wrap gap-2 p-2">
      <Button size="sm" :disabled="sortStatus || busy" @click="handleAddGroup">
        添加
      </Button>
      <Button v-if="!sortStatus" variant="outline" size="sm" :disabled="busy" @click="sortStatus = true">
        排序
      </Button>
      <template v-else>
        <Button size="sm" :disabled="sortSaving" :aria-busy="sortSaving" @click="handleSaveSort">
          <span v-if="sortSaving" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
          保存排序
        </Button>
        <Button variant="outline" size="sm" :disabled="sortSaving" @click="cancelSort">
          {{ text.cancelSort }}
        </Button>
      </template>
      <Button v-if="navigation.undoCount" variant="outline" size="sm" :disabled="busy || sortStatus || !navigation.canUndo" @click="undoChange">
        {{ text.undo }}
      </Button>
    </div>
    <div class="mt-4 min-h-0 w-full flex-1 overflow-auto rounded-xl border border-border bg-muted/40">
      <VueDraggable v-model="groups" item-key="id" :animation="300" class="space-y-2.5 p-2.5" :disabled="!sortStatus || sortSaving" filter="button">
        <div v-for="(item, index) in groups" :key="item.id" class="rounded-xl border border-border bg-card p-3 text-card-foreground">
          <div class="flex items-center gap-2" :class="sortStatus ? 'cursor-move' : ''">
            <SvgIcon class="shrink-0 text-xl text-muted-foreground" :icon="item.icon || 'material-symbols-ad-group-outline-rounded'" />
            <span class="flex-1 break-all">{{ item.title }}</span>
            <template v-if="sortStatus">
              <Button variant="outline" size="icon" class="size-8" :disabled="index === 0 || sortSaving" :aria-label="text.up" @click="groups = moved(groups, index, -1)">
                ↑
              </Button>
              <Button variant="outline" size="icon" class="size-8" :disabled="index === groups.length - 1 || sortSaving" :aria-label="text.down" @click="groups = moved(groups, index, 1)">
                ↓
              </Button>
            </template>
            <template v-else>
              <Button variant="outline" size="icon" class="size-8" aria-label="编辑" :disabled="deleting !== null" @click="handleEditGroup(item)">
                <SvgIcon icon="basil:edit-solid" />
              </Button>
              <Button variant="destructive" size="icon" class="size-8" aria-label="删除" :disabled="deleting !== null" :aria-busy="deleting === item.id" @click="handleDelete(item)">
                <span v-if="deleting === item.id" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
                <SvgIcon v-else icon="material-symbols:delete" />
              </Button>
            </template>
          </div>
        </div>
      </VueDraggable>
    </div>
    <RoundCardModal v-model:show="modalShow" class="sm:max-w-[400px]" :mask-closable="!saving" :close-on-esc="!saving" :closable="!saving" :title="editing ? '编辑' : '添加'">
      <form id="group-editor-form" ref="formRef" novalidate :aria-busy="saving" @submit.prevent="handleSaveGroup">
        <div class="space-y-2">
          <Label for="group-title">分组名称</Label>
          <Input id="group-title" v-model="draft.title" :maxlength="20" required :disabled="saving" :aria-invalid="!!titleError" :aria-describedby="titleError ? 'group-title-error' : undefined" @update:model-value="titleError = ''" />
          <p v-if="titleError" id="group-title-error" class="text-sm text-destructive" role="alert">
            {{ titleError }}
          </p>
        </div>
      </form>
      <template #footer>
        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" :disabled="saving" @click="modalShow = false">
            取消
          </Button>
          <Button type="submit" form="group-editor-form" size="sm" :disabled="saving" :aria-busy="saving">
            <span v-if="saving" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
            确定
          </Button>
        </div>
      </template>
    </RoundCardModal>
  </div>
</template>
