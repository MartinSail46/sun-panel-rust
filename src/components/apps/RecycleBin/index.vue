<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { listTrash, purgeTrash, restoreTrash } from '@/api/panel/trash'
import type { TrashEntry, TrashRef } from '@/api/panel/trash'
import { RoundCardModal, SvgIcon } from '@/components/common'
import AppSelect from '@/components/common/AppSelect.vue'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { toast } from '@/lib/feedback'
import { useNavigationStore } from '@/store/modules/navigation'
import { isRequestCancelled, isUncertainWrite, RequestError } from '@/utils/requestLifetime'

const emit = defineEmits<{ busy: [value: boolean] }>()
const navigation = useNavigationStore()
const entries = ref<TrashEntry[]>([])
const loading = ref(false)
const loadError = ref('')
const notice = ref('')
const selected = ref<TrashEntry>()
const action = ref<'restore' | 'purge'>('restore')
const modal = ref(false)
const preparing = ref(false)
const prepared = ref(false)
const writing = ref(false)
const actionError = ref('')
const uncertain = ref(false)
const targetGroupId = ref<number>()
const busy = computed(() => preparing.value || writing.value)
const navigationBusy = computed(() => navigation.busy)
const blocked = computed(() => busy.value || navigationBusy.value)
const text = {
  title: '回收站', description: '删除的应用和分组保留于此。恢复分组时，会一起恢复随该分组删除的应用。',
  permanentHint: '永久删除后无法恢复，请确认不再需要。', refresh: '刷新', loading: '正在读取回收站…',
  empty: '回收站是空的', emptyHint: '删除的应用和分组会显示在这里，方便找回。',
  group: '分组', item: '应用', applications: '个应用', deleted: '删除于', originalGroup: '原分组', noGroup: '原分组已不可用',
  restore: '恢复', purge: '永久删除', cancel: '取消', restoreTitle: '恢复到导航', purgeTitle: '永久删除',
  target: '恢复到分组', chooseTarget: '请选择目标分组', targetRequired: '原分组不可用，请选择一个现有分组。',
  noGroups: '暂无可用分组。请先在分组管理中新建分组，再恢复此应用。', preparing: '正在检查可用分组…',
  groupRestore: '该分组及随它删除的应用将一起恢复到导航。', groupPurge: '该分组及随它删除的应用将被永久删除，此操作无法撤销。',
  itemPurge: '此应用将被永久删除，此操作无法撤销。', restored: '已恢复到导航', purged: '已永久删除',
  error: '操作失败，请重试。', loadFail: '无法读取回收站，请重试。', prepareFail: '无法读取可用分组，请重新检查后恢复。',
  retry: '重试', verify: '重新检查', verifyHint: '正在核对回收站和导航，请勿重复提交。',
  refreshed: '已重新读取当前状态，请核对结果后再操作。', refreshAfterRestore: '已恢复，但首页或回收站刷新失败。请刷新页面，不要重复恢复。',
  refreshAfterPurge: '已永久删除，但回收站刷新失败。请刷新列表，不要重复删除。',
  waiting: '导航有操作等待完成或核对，请先完成该操作。', count: '条记录',
}
const groupOptions = computed(() => navigation.groups.filter(group => group.id !== undefined).map(group => ({ label: group.title || '', value: group.id! })))
const missingOriginalGroup = computed(() => !navigation.groups.some(group => group.id === selected.value?.groupId))
const canSubmit = computed(() => !blocked.value && !uncertain.value && (action.value === 'purge' || selected.value?.kind === 'group' || (prepared.value && groupOptions.value.some(group => group.value === targetGroupId.value))))
let listController: AbortController | undefined
let listGeneration = 0
let alive = true

watch(busy, value => emit('busy', value), { immediate: true })

function entryKey(entry: TrashRef) {
  return `${entry.kind}:${entry.id}:${entry.deletionId || ''}`
}
function reference(entry: TrashEntry): TrashRef {
  return { kind: entry.kind, id: entry.id, ...(entry.deletionId ? { deletionId: entry.deletionId } : {}) }
}
function message(cause: unknown, fallback: string) {
  if (isUncertainWrite(cause)) return '操作结果尚未确认，请先刷新检查，避免重复提交。'
  if (cause instanceof RequestError && cause.kind === 'timeout') return '请求超时，请稍后重试'
  if (cause instanceof RequestError && cause.kind === 'network') return '网络错误，请稍后重试'
  if (cause instanceof Error) return cause.message
  if (cause && typeof cause === 'object' && 'msg' in cause) return String(cause.msg || fallback)
  return fallback
}
function formatDate(value: string) {
  // SQLite's UTC timestamp has no suffix in some existing databases.
  const normalized = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?$/.test(value) ? `${value.replace(' ', 'T')}Z` : value
  const date = new Date(normalized)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString('zh-CN', { dateStyle: 'medium', timeStyle: 'short' })
}
async function loadEntries(): Promise<boolean> {
  listController?.abort()
  const controller = new AbortController()
  listController = controller
  const generation = ++listGeneration
  loading.value = true
  loadError.value = ''
  try {
    const response = await listTrash({ signal: controller.signal, silent: true })
    if (generation !== listGeneration || !alive) return false
    if (response.code !== 0) throw response
    entries.value = response.data.list
    return true
  }
  catch (cause) {
    if (generation === listGeneration && alive && !isRequestCancelled(cause))
      loadError.value = message(cause, text.loadFail)
    return false
  }
  finally {
    if (generation === listGeneration) loading.value = false
  }
}
async function prepareGroups() {
  preparing.value = true
  prepared.value = false
  actionError.value = ''
  try {
    await navigation.refresh()
    if (!alive) return
    const original = selected.value?.groupId
    targetGroupId.value = navigation.groups.some(group => group.id === original) ? original : undefined
    prepared.value = true
  }
  catch (cause) { if (alive) actionError.value = message(cause, text.prepareFail) }
  finally { preparing.value = false }
}
async function refreshView() {
  if (busy.value || navigation.saving || loading.value) return
  preparing.value = true
  try {
    await Promise.all([loadEntries(), navigation.refresh().catch(cause => toast.error(message(cause, text.loadFail)))])
  }
  finally { preparing.value = false }
}
async function openAction(entry: TrashEntry, value: 'restore' | 'purge') {
  if (blocked.value || loading.value) return
  selected.value = entry
  action.value = value
  actionError.value = ''
  uncertain.value = false
  targetGroupId.value = undefined
  prepared.value = false
  modal.value = true
  if (value === 'restore' && entry.kind === 'item') await prepareGroups()
}
function updateModal(visible: boolean) {
  if (!busy.value) modal.value = visible
}
function updateTarget(value: string | number) {
  targetGroupId.value = Number(value)
}
async function verifyUncertain() {
  if (busy.value) return
  preparing.value = true
  try {
    const [listResult] = await Promise.all([loadEntries(), navigation.refresh().then(() => true, () => false)])
    if (!alive) return
    if (listResult) {
      uncertain.value = false
      notice.value = text.refreshed
      actionError.value = ''
      if (!entries.value.some(entry => entryKey(entry) === entryKey(selected.value!))) modal.value = false
    }
  }
  finally { preparing.value = false }
}
async function submit() {
  const entry = selected.value
  if (!entry || !canSubmit.value) return
  writing.value = true
  actionError.value = ''
  notice.value = ''
  const operation = action.value
  try {
    await navigation.withMutation(async () => {
      const response = operation === 'restore'
        ? await restoreTrash({ ...reference(entry), ...(entry.kind === 'item' ? { targetGroupId: targetGroupId.value } : {}) }, { silent: true })
        : await purgeTrash(reference(entry), { silent: true })
      if (response.code !== 0) throw response
    })
    // Acknowledged writes stay successful even if a later read fails.
    entries.value = entries.value.filter(value => entryKey(value) !== entryKey(entry))
    modal.value = false
    toast.success(operation === 'restore' ? text.restored : text.purged)
    const [listResult, navigationResult] = await Promise.all([
      loadEntries(),
      operation === 'restore' ? navigation.refresh().then(() => true, () => false) : Promise.resolve(true),
    ])
    if (alive && (!listResult || !navigationResult)) {
      notice.value = operation === 'restore' ? text.refreshAfterRestore : text.refreshAfterPurge
      toast.warning(notice.value)
    }
  }
  catch (cause) {
    if (!alive) return
    actionError.value = message(cause, text.error)
    if (isUncertainWrite(cause)) {
      uncertain.value = true
      notice.value = '操作结果尚未确认，请先刷新检查，避免重复提交。'
      // Read both surfaces to reconcile; never replay an uncertain write.
      const [listResult] = await Promise.all([loadEntries(), navigation.refresh().then(() => true, () => false)])
      if (listResult && !entries.value.some(value => entryKey(value) === entryKey(entry))) modal.value = false
    }
  }
  finally { writing.value = false }
}

onMounted(() => { void loadEntries() })
onBeforeUnmount(() => {
  alive = false
  listGeneration++
  listController?.abort()
  emit('busy', false)
})
</script>

<template>
  <div class="space-y-4 p-2 text-card-foreground sm:p-4">
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0 space-y-1">
        <h2 class="text-base font-semibold">
          {{ text.title }} <span v-if="!loading && !loadError" class="ml-1 text-xs font-normal text-muted-foreground">{{ entries.length }} {{ text.count }}</span>
        </h2>
        <p class="text-sm leading-6 text-muted-foreground">
          {{ text.description }}
        </p>
      </div>
      <Button variant="outline" size="sm" class="shrink-0" :disabled="busy || navigation.saving || loading" :aria-busy="loading" @click="refreshView">
        <SvgIcon :class="{ 'animate-spin': loading }" icon="lucide:refresh-cw" />{{ text.refresh }}
      </Button>
    </div>
    <p v-if="notice" class="rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-sm" role="status">
      {{ notice }}
    </p>
    <div v-if="navigationBusy" class="space-y-2 rounded-lg border bg-muted/50 p-3 text-sm text-muted-foreground" role="status">
      <p>{{ text.waiting }}</p>
      <Button v-if="navigation.needsVerification" variant="outline" size="sm" :disabled="busy || navigation.saving || loading" @click="refreshView">
        {{ text.verify }}
      </Button>
    </div>
    <div v-if="loadError" class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-destructive/30 bg-destructive/10 p-3" role="alert">
      <p class="text-sm text-destructive">
        {{ loadError }}
      </p>
      <Button variant="outline" size="sm" :disabled="blocked || loading" @click="loadEntries">
        {{ text.retry }}
      </Button>
    </div>
    <div v-if="loading && !entries.length" class="flex min-h-40 items-center justify-center gap-2 text-sm text-muted-foreground" role="status">
      <span class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />{{ text.loading }}
    </div>
    <div v-else-if="!entries.length && !loadError" class="rounded-xl border border-dashed p-8 text-center">
      <SvgIcon icon="lucide:trash-2" class="mx-auto mb-3 text-3xl text-muted-foreground" />
      <p class="font-medium">
        {{ text.empty }}
      </p>
      <p class="mt-2 text-sm text-muted-foreground">
        {{ text.emptyHint }}
      </p>
    </div>
    <ul v-else-if="entries.length" class="space-y-2" :aria-busy="loading">
      <li v-for="entry in entries" :key="entryKey(entry)" class="rounded-xl border bg-card p-3">
        <div class="flex gap-3">
          <span class="flex size-9 shrink-0 items-center justify-center rounded-lg bg-muted text-lg text-muted-foreground"><SvgIcon :icon="entry.kind === 'group' ? 'lucide:folder' : 'lucide:app-window'" /></span>
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-x-2 gap-y-1">
              <p class="break-all text-sm font-medium">
                {{ entry.title }}
              </p>
              <span class="rounded border px-1.5 py-0.5 text-[11px] text-muted-foreground">{{ entry.kind === 'group' ? text.group : text.item }}</span>
            </div>
            <p v-if="entry.kind === 'item'" class="mt-1 break-all text-xs text-muted-foreground">
              {{ text.originalGroup }}：{{ entry.groupTitle || text.noGroup }}
            </p>
            <p v-else class="mt-1 text-xs text-muted-foreground">
              {{ entry.itemCount }} {{ text.applications }}
            </p>
            <p class="mt-1 text-xs text-muted-foreground">
              {{ text.deleted }} {{ formatDate(entry.deletedAt) }}
            </p>
          </div>
        </div>
        <div class="mt-3 flex flex-wrap justify-end gap-2">
          <Button variant="outline" size="sm" :disabled="blocked || loading" @click="openAction(entry, 'restore')">
            <SvgIcon icon="lucide:undo-2" />{{ text.restore }}
          </Button>
          <Button variant="ghost" size="sm" class="text-destructive hover:text-destructive" :disabled="blocked || loading" @click="openAction(entry, 'purge')">
            <SvgIcon icon="lucide:trash-2" />{{ text.purge }}
          </Button>
        </div>
      </li>
    </ul>
    <p v-if="entries.length" class="text-xs text-muted-foreground">
      {{ text.permanentHint }}
    </p>
    <RoundCardModal :show="modal" :title="action === 'restore' ? text.restoreTitle : text.purgeTitle" :closable="!busy" :mask-closable="!busy" :close-on-esc="!busy" class="sm:max-w-md" @update:show="updateModal">
      <div v-if="selected" class="space-y-4" :aria-busy="busy">
        <p class="break-all font-medium">
          {{ selected.title }}
        </p>
        <p v-if="action === 'purge'" class="text-sm leading-6 text-muted-foreground">
          {{ selected.kind === 'group' ? text.groupPurge : text.itemPurge }}
        </p>
        <p v-else-if="selected.kind === 'group'" class="text-sm leading-6 text-muted-foreground">
          {{ text.groupRestore }} <span class="whitespace-nowrap">({{ selected.itemCount }} {{ text.applications }})</span>
        </p>
        <template v-else>
          <p v-if="preparing" class="text-sm text-muted-foreground" role="status">
            {{ uncertain ? text.verifyHint : text.preparing }}
          </p>
          <div v-else-if="prepared" class="space-y-2">
            <Label for="trash-target-group">{{ text.target }}</Label>
            <AppSelect id="trash-target-group" :model-value="targetGroupId" :options="groupOptions" :placeholder="text.chooseTarget" :disabled="writing || !groupOptions.length || uncertain" @update:model-value="updateTarget" />
            <p v-if="!groupOptions.length" class="text-sm leading-6 text-muted-foreground">
              {{ text.noGroups }}
            </p>
            <p v-else-if="missingOriginalGroup" class="text-sm text-muted-foreground">
              {{ text.targetRequired }}
            </p>
          </div>
        </template>
        <div v-if="actionError" class="space-y-2 rounded-lg border border-destructive/30 bg-destructive/10 p-3" role="alert">
          <p class="text-sm text-destructive">
            {{ actionError }}
          </p>
          <Button v-if="uncertain" variant="outline" size="sm" :disabled="busy" @click="verifyUncertain">
            {{ text.verify }}
          </Button>
          <Button v-else-if="action === 'restore' && selected.kind === 'item' && !prepared" variant="outline" size="sm" :disabled="busy" @click="prepareGroups">
            {{ text.retry }}
          </Button>
        </div>
      </div>
      <template #footer>
        <div class="flex w-full justify-end gap-2">
          <Button variant="outline" :disabled="busy" @click="modal = false">
            {{ text.cancel }}
          </Button>
          <Button :variant="action === 'purge' ? 'destructive' : 'default'" :disabled="!canSubmit" :aria-busy="writing" @click="submit">
            <span v-if="writing" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
            <SvgIcon v-else :icon="action === 'restore' ? 'lucide:undo-2' : 'lucide:trash-2'" />{{ action === 'restore' ? text.restore : text.purge }}
          </Button>
        </div>
      </template>
    </RoundCardModal>
  </div>
</template>
