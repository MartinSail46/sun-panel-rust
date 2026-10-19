import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'
import { getNavigation, moveNavigation } from '@/api/panel/navigation'
import { deletes as deleteItems } from '@/api/panel/itemIcon'
import { deletes as deleteGroups, saveSort } from '@/api/panel/itemIconGroup'
import { restoreTrash } from '@/api/panel/trash'
import type { Response } from '@/utils/request'
import { isRequestCancelled, isUncertainWrite } from '@/utils/requestLifetime'
import { cloneNavigation, createNavigationMover } from '@/utils/navigationMove'
import type { NavigationMoveResult, NavigationSnapshot } from '@/utils/navigationMove'
import { createNavigationHistory } from '@/utils/navigationHistory'
import type { HistoryTrashRef, NavigationUndoAction, NavigationUndoRecord } from '@/utils/navigationHistory'

export interface NavigationGroup extends Panel.ItemIconGroup {
  items: Panel.ItemInfo[]
}
interface WriteData { revision: number; trash?: HistoryTrashRef[] }

export const useNavigationStore = defineStore('navigation', () => {
  const groups = ref<NavigationGroup[]>([])
  const revision = ref(0)
  const loading = ref(false)
  const loaded = ref(false)
  const error = ref('')
  const cancelled = ref(false)
  const moveSaving = ref(false)
  const mutationSaving = ref(false)
  const moveVerification = ref(false)
  const mutationVerification = ref(false)
  const saving = computed(() => moveSaving.value || mutationSaving.value)
  const needsVerification = computed(() => moveVerification.value || mutationVerification.value)
  const busy = computed(() => saving.value || needsVerification.value || loading.value)
  const undoRecord = shallowRef<NavigationUndoRecord>()
  const undoCount = ref(0)
  const canUndo = computed(() => !!undoRecord.value && !busy.value)
  const history = createNavigationHistory((record, count) => { undoRecord.value = record; undoCount.value = count })
  let historyWriting = false
  let mutationOutcomeUnknown = false
  let pendingHistory: { action?: NavigationUndoAction; undo?: boolean } | undefined
  let pending: Promise<void> | undefined
  let moving: Promise<NavigationMoveResult> | undefined
  let mutation: Promise<unknown> | undefined
  let controller: AbortController | undefined
  let requested = false

  async function read(signal?: AbortSignal): Promise<NavigationSnapshot<NavigationGroup>> {
    const response = await getNavigation<Common.ListResponse<NavigationGroup[]>>({ signal, silent: true })
    if (response.code !== 0)
      throw Object.assign(new Error(response.msg || '导航加载失败，请重试'), response)
    if (!Number.isSafeInteger(response.revision) || response.revision! < 0)
      throw new Error('导航数据版本缺失或无效，请刷新后重试')
    return { groups: response.data.list.map(group => ({ ...group, items: group.items || [] })), revision: response.revision! }
  }

  function apply(snapshot: NavigationSnapshot<NavigationGroup>) {
    if (!historyWriting) history.reconcile(snapshot.revision)
    groups.value = snapshot.groups
    revision.value = snapshot.revision
    loaded.value = true
    error.value = ''
    cancelled.value = false
  }

  const mover = createNavigationMover<NavigationGroup>({
    read: () => read(),
    write: async (move) => {
      const response = await moveNavigation(move, { silent: true })
      if (response.code !== 0)
        throw Object.assign(new Error(response.msg || '应用移动失败，请重试'), response)
      if (!Number.isSafeInteger(response.revision) || response.revision! < 0)
        throw new Error('导航数据版本缺失或无效，请刷新后重试')
      return response.revision!
    },
    apply,
    onStateChange: (state) => {
      moveSaving.value = state.saving
      moveVerification.value = state.needsVerification
    },
  })

  function busyError() {
    return Object.assign(new Error('请先完成上次修改的保存或结果确认'), { code: 'NAVIGATION_BUSY' })
  }

  function trackMove(task: Promise<NavigationMoveResult>) {
    moving = task
    const clear = () => { if (moving === task) moving = undefined }
    void task.then(clear, clear)
    return task
  }

  function snapshot(): NavigationSnapshot<NavigationGroup> {
    return { groups: cloneNavigation(groups.value), revision: revision.value }
  }

  function finishHistory(result: NavigationMoveResult) {
    if (result === 'reloaded') history.clear()
    else if (pendingHistory?.undo) history.completeUndo(revision.value)
    else if (pendingHistory?.action) history.record(pendingHistory.action, revision.value)
    pendingHistory = undefined
  }

  function move(itemId: number, targetGroupId: number, targetIndex: number, base = snapshot()): Promise<NavigationMoveResult> {
    if (busy.value || moving || mutation) return Promise.reject(busyError())
    const source = base.groups.find(group => group.items.some(item => item.id === itemId))
    const sourceIndex = source?.items.findIndex(item => item.id === itemId) ?? -1
    const inverse: NavigationUndoAction | undefined = source?.id && sourceIndex >= 0 && (source.id !== targetGroupId || sourceIndex !== targetIndex)
      ? { kind: 'move', itemId, targetGroupId: source.id, targetIndex: sourceIndex, title: source.items[sourceIndex]!.title }
      : undefined
    moveSaving.value = true
    historyWriting = true
    requested = false
    controller?.abort()
    pendingHistory = { action: inverse }
    return trackMove((async () => {
      if (pending) await pending.catch(() => {})
      cancelled.value = false
      try {
        const result = await mover.move(base, { itemId, targetGroupId, targetIndex })
        finishHistory(result)
        return result
      }
      catch (cause) {
        if (!mover.state.needsVerification) { history.clear(); pendingHistory = undefined }
        throw cause
      }
      finally { moveSaving.value = mover.state.saving; historyWriting = false }
    })())
  }

  function verifyMove(): Promise<NavigationMoveResult> {
    if (moving) return moving
    if (mutation) return Promise.reject(busyError())
    historyWriting = true
    return trackMove((async () => {
      try {
        if (mutationVerification.value) {
          apply(await read())
          mutationVerification.value = false
          if (mutationOutcomeUnknown) history.clear()
          else history.reconcile(revision.value)
          const result = mutationOutcomeUnknown ? 'reloaded' : 'verified'
          mutationOutcomeUnknown = false
          return result
        }
        const result = await mover.verifyMove()
        finishHistory(result)
        return result
      }
      finally { historyWriting = false }
    })())
  }

  function mutate<T>(run: () => Promise<T>): Promise<T> {
    if (busy.value || moving || mutation) return Promise.reject(busyError())
    mutationSaving.value = true
    historyWriting = true
    const task = run().finally(() => { mutationSaving.value = false; historyWriting = false })
    mutation = task
    const clear = () => { if (mutation === task) mutation = undefined }
    void task.then(clear, clear)
    return task
  }

  function withMutation<T>(run: () => Promise<T>) {
    return mutate(async () => {
      history.clear()
      try { return await run() }
      catch (cause) {
        if (isUncertainWrite(cause)) { mutationVerification.value = true; mutationOutcomeUnknown = true }
        throw cause
      }
    })
  }

  async function writeAndRefresh(write: () => Promise<Response<WriteData>>, acknowledged: (data: WriteData, savedRevision: number) => void) {
    let response: Response<WriteData>
    try {
      response = await write()
      if (response.code !== 0)
        throw Object.assign(new Error(response.msg || '导航保存失败，请重试'), response)
    }
    catch (cause) {
      history.clear()
      mutationVerification.value = isUncertainWrite(cause)
      mutationOutcomeUnknown = isUncertainWrite(cause)
      try { apply(await read()) }
      catch { mutationVerification.value = true }
      throw cause
    }
    const savedRevision = response.revision ?? response.data?.revision
    if (!Number.isSafeInteger(savedRevision) || savedRevision! < 0) {
      history.clear()
      mutationVerification.value = true
      mutationOutcomeUnknown = true
      throw new Error('导航数据版本缺失或无效，请刷新后重试')
    }
    revision.value = savedRevision!
    acknowledged(response.data, savedRevision!)
    // An acknowledged write remains successful even when the subsequent read fails.
    try {
      apply(await read())
      history.reconcile(revision.value)
    }
    catch (cause) {
      error.value = cause instanceof Error ? cause.message : '导航加载失败，请重试'
      mutationVerification.value = true
    }
  }

  function deleteItem(id: number) {
    return mutate(async () => {
      const source = groups.value.find(group => group.items.some(item => item.id === id))
      const index = source?.items.findIndex(item => item.id === id) ?? -1
      const item = source?.items[index]
      await writeAndRefresh(() => deleteItems<WriteData>([id], { silent: true }, revision.value), (data, savedRevision) => {
        groups.value = groups.value.map(group => ({ ...group, items: group.items.filter(item => item.id !== id) }))
        const entry = data.trash?.find(entry => entry.kind === 'item' && entry.id === id)
        if (entry) history.record({ kind: 'delete', entry, title: item?.title || '', position: source?.id ? { groupId: source.id, index } : undefined }, savedRevision)
        else history.clear()
      })
    })
  }

  function deleteGroup(id: number) {
    return mutate(async () => {
      const group = groups.value.find(group => group.id === id)
      await writeAndRefresh(() => deleteGroups<WriteData>([id], { silent: true }, revision.value), (data, savedRevision) => {
        groups.value = groups.value.filter(group => group.id !== id)
        const entry = data.trash?.find(entry => entry.kind === 'group' && entry.id === id)
        if (entry) history.record({ kind: 'delete', entry, title: group?.title || '' }, savedRevision)
        else history.clear()
      })
    })
  }

  function sortGroups(order: Common.SortItemRequest[]) {
    return mutate(async () => {
      const inverse = groups.value.map((group, index) => ({ id: group.id!, sort: group.sort ?? index }))
      await writeAndRefresh(() => saveSort<WriteData>(order, { silent: true }, revision.value), (_data, savedRevision) => {
        applyGroupOrder(order)
        history.record({ kind: 'sort', order: inverse }, savedRevision)
      })
    })
  }

  function applyGroupOrder(order: Common.SortItemRequest[]) {
    const positions = new Map(order.map(item => [item.id, item.sort]))
    groups.value = groups.value.map(group => ({ ...group, sort: positions.get(group.id!) ?? group.sort }))
      .sort((left, right) => (left.sort ?? 0) - (right.sort ?? 0) || (left.createTime || '').localeCompare(right.createTime || '') || left.id! - right.id!)
  }

  function undo() {
    return mutate(async () => {
      const record = undoRecord.value
      if (!record) return
      if (record.revision !== revision.value) {
        history.clear()
        throw Object.assign(new Error('导航已更新，请刷新后再撤销'), { code: 1409 })
      }
      const action = record.action
      if (action.kind === 'move') {
        pendingHistory = { undo: true }
        try {
          const result = await mover.move(snapshot(), action)
          finishHistory(result)
        }
        catch (cause) {
          if (!mover.state.needsVerification) { history.clear(); pendingHistory = undefined }
          throw cause
        }
      }
      else {
        await writeAndRefresh(
          () => action.kind === 'delete'
            ? restoreTrash<WriteData>({ ...action.entry, revision: record.revision, ...(action.position ? { targetGroupId: action.position.groupId, targetIndex: action.position.index } : {}) }, { silent: true })
            : saveSort<WriteData>(action.order, { silent: true }, record.revision),
          (_data, savedRevision) => { if (action.kind === 'sort') applyGroupOrder(action.order); history.completeUndo(savedRevision) },
        )
      }
    })
  }

  function refresh(): Promise<void> {
    if (moving) return moving.then(() => refresh(), () => refresh())
    if (mutation) return mutation.then(() => refresh(), () => refresh())
    if (needsVerification.value) return verifyMove().then(() => {})
    if (pending && controller?.signal.aborted) return pending.then(() => refresh())
    requested = true
    if (pending) return pending
    loading.value = true
    error.value = ''
    cancelled.value = false
    pending = (async () => {
      try {
        while (requested) {
          requested = false
          controller = new AbortController()
          apply(await read(controller.signal))
        }
      }
      catch (cause) {
        if (isRequestCancelled(cause)) { cancelled.value = true; return }
        error.value = cause instanceof Error ? cause.message : '导航加载失败，请重试'
        throw cause
      }
      finally { loading.value = false; pending = undefined; controller = undefined }
    })()
    return pending
  }

  function load() { return loaded.value ? Promise.resolve() : (pending || refresh()) }
  function cancel() { requested = false; controller?.abort() }
  return { groups, revision, loading, loaded, error, cancelled, saving, needsVerification, busy, undoRecord, undoCount, canUndo, undo, deleteItem, deleteGroup, sortGroups, withMutation, load, refresh, cancel, snapshot, move, verifyMove }
})
