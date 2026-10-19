import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { runInNewContext } from 'node:vm'
import { createPinia, defineStore, setActivePinia } from 'pinia'
import * as Vue from 'vue'
import ts from 'typescript'
import * as navigationMove from '../src/utils/navigationMove.ts'
import * as navigationHistory from '../src/utils/navigationHistory.ts'
import { isRequestCancelled, isUncertainWrite, RequestError } from '../src/utils/requestLifetime.ts'

interface Item { id: number; title: string; sort: number; itemIconGroupId: number }
interface Group { id: number; title: string; sort: number; items: Item[] }
interface TrashRef { kind: 'item' | 'group'; id: number; deletionId: string; targetGroupId?: number; targetIndex?: number; revision?: number }
interface Store {
  groups: Group[]; revision: number; undoCount: number; canUndo: boolean; busy: boolean; needsVerification: boolean; error: string
  undoRecord?: { action: { kind: string }; revision: number }
  load: () => Promise<void>; refresh: () => Promise<void>; verifyMove: () => Promise<string>; undo: () => Promise<void>
  move: (id: number, group: number, index: number) => Promise<string>
  deleteItem: (id: number) => Promise<void>; deleteGroup: (id: number) => Promise<void>
  sortGroups: (order: { id: number; sort: number }[]) => Promise<void>
}
const source = readFileSync(new URL('../src/store/modules/navigation/index.ts', import.meta.url), 'utf8')
const javascript = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
const plain = <T>(value: T): T => JSON.parse(JSON.stringify(value))
const layout = (groups: Group[]) => plain(groups.map(group => [group.id, group.items.map(item => item.id)]))

function harness() {
  let remote: Group[] = [
    { id: 1, title: 'Apps', sort: 0, items: [11,12,13].map((id, sort) => ({ id, title: `App ${id}`, sort, itemIconGroupId: 1 })) },
    { id: 2, title: 'Media', sort: 1, items: [{ id: 21, title: 'Media app', sort: 0, itemIconGroupId: 2 }] },
    { id: 3, title: 'Empty', sort: 2, items: [] },
  ]
  let revision = 10
  let failReads = 0
  let loseRestoreAcknowledgement = false
  const trash = new Map<string, { entry: TrashRef; value: Group | Item }>()
  const writes: Array<{ kind: string; revision: number | undefined; payload: unknown }> = []
  const response = (data: unknown = { revision }) => ({ code: 0, revision, data })
  const cas = (expected?: number) => {
    if (expected !== revision) throw Object.assign(new Error('Navigation changed'), { code: 1409, currentRevision: revision })
  }
  const sortRemote = (order: { id: number; sort: number }[]) => {
    for (const entry of order) remote.find(group => group.id === entry.id)!.sort = entry.sort
    remote.sort((left, right) => left.sort - right.sort || left.id - right.id)
  }
  const remove = async (kind: 'item' | 'group', ids: number[], _options: unknown, expected?: number) => {
    writes.push({ kind: `delete-${kind}`, revision: expected, payload: plain(ids) })
    cas(expected)
    const entries: TrashRef[] = []
    for (const id of ids) {
      const group = remote.find(group => kind === 'group' ? group.id === id : group.items.some(item => item.id === id))!
      const value = kind === 'group' ? group : group.items.find(item => item.id === id)!
      const entry: TrashRef = { kind, id, deletionId: `batch-${revision}-${id}` }
      trash.set(`${kind}-${id}`, { entry, value: plain(value) })
      if (kind === 'group') remote = remote.filter(group => group.id !== id)
      else group.items = group.items.filter(item => item.id !== id)
      entries.push(entry)
    }
    revision++
    return response({ revision, trash: entries })
  }
  const dependencies: Record<string, unknown> = {
    pinia: { defineStore }, vue: Vue,
    '@/api/panel/navigation': {
      getNavigation: async () => {
        if (failReads > 0) { failReads--; throw new Error('Follow-up read unavailable') }
        return response({ list: plain(remote), count: remote.length })
      },
      moveNavigation: async (move: navigationMove.NavigationMoveRequest) => {
        writes.push({ kind: 'move', revision: move.revision, payload: plain(move) })
        cas(move.revision)
        remote = navigationMove.applyNavigationMove(remote, move)
        revision++
        return response()
      },
    },
    '@/api/panel/itemIcon': { deletes: (ids: number[], options: unknown, expected?: number) => remove('item', ids, options, expected) },
    '@/api/panel/itemIconGroup': {
      deletes: (ids: number[], options: unknown, expected?: number) => remove('group', ids, options, expected),
      saveSort: async (order: { id: number; sort: number }[], _options: unknown, expected?: number) => {
        writes.push({ kind: 'sort', revision: expected, payload: plain(order) })
        cas(expected)
        sortRemote(order)
        revision++
        return response()
      },
    },
    '@/api/panel/trash': {
      restoreTrash: async (entry: TrashRef) => {
        writes.push({ kind: 'restore', revision: entry.revision, payload: plain(entry) })
        cas(entry.revision)
        const saved = trash.get(`${entry.kind}-${entry.id}`)
        if (!saved || saved.entry.deletionId !== entry.deletionId) throw Object.assign(new Error('Deleted entry changed'), { code: 1409 })
        if (entry.kind === 'group') {
          remote.push(plain(saved.value as Group))
          remote.sort((left, right) => left.sort - right.sort || left.id - right.id)
        }
        else {
          const item = plain(saved.value as Item)
          const group = remote.find(group => group.id === (entry.targetGroupId ?? item.itemIconGroupId))!
          item.itemIconGroupId = group.id
          if (entry.targetIndex !== undefined) group.items.splice(entry.targetIndex, 0, item)
          else { group.items.push(item); group.items.sort((left, right) => left.sort - right.sort || left.id - right.id) }
          group.items.forEach((item, index) => { item.sort = index })
        }
        trash.delete(`${entry.kind}-${entry.id}`)
        revision++
        if (loseRestoreAcknowledgement) { loseRestoreAcknowledgement = false; throw new RequestError('timeout', 'write') }
        return response()
      },
    },
    '@/utils/requestLifetime': { isRequestCancelled, isUncertainWrite },
    '@/utils/navigationMove': navigationMove,
    '@/utils/navigationHistory': navigationHistory,
  }
  const module = { exports: {} as { useNavigationStore: () => Store } }
  runInNewContext(javascript, {
    module, exports: module.exports, AbortController, Error, structuredClone,
    require(name: string) {
      if (name in dependencies) return dependencies[name]
      throw new Error(`Unexpected navigation store dependency: ${name}`)
    },
  })
  setActivePinia(createPinia())
  const store = module.exports.useNavigationStore()
  return {
    store, writes,
    get remote() { return plain(remote) },
    get revision() { return revision },
    failNextRead() { failReads++ },
    loseNextRestoreAcknowledgement() { loseRestoreAcknowledgement = true },
    externalChange() { remote[0]!.title = 'Changed in another page'; revision++ },
  }
}

test('actual navigation store undoes move, delete and group sort in reverse order while rebasing revisions', async () => {
  const ui = harness()
  await ui.store.load()
  const original = layout(ui.remote)
  await ui.store.move(11, 2, 1)
  await ui.store.deleteItem(12)
  await ui.store.sortGroups([{ id: 3, sort: 0 }, { id: 2, sort: 1 }, { id: 1, sort: 2 }])
  assert.equal(ui.store.undoCount, 3)
  for (const remaining of [2,1,0]) {
    await ui.store.undo()
    assert.equal(ui.store.undoCount, remaining)
    assert.equal(ui.store.revision, ui.revision)
    assert.deepEqual(layout(ui.store.groups), layout(ui.remote))
    if (remaining) assert.equal(ui.store.undoRecord?.revision, ui.revision)
  }
  assert.deepEqual(layout(ui.remote), original)
  assert.deepEqual(ui.writes.map(write => write.kind), ['move', 'delete-item', 'sort', 'sort', 'restore', 'move'])
  assert.equal(ui.store.canUndo, false)
})

test('actual navigation store recovers a deleted group and its applications after undoing a later move', async () => {
  const ui = harness()
  await ui.store.load()
  const original = layout(ui.remote)
  await ui.store.deleteGroup(2)
  await ui.store.move(11, 3, 0)
  await ui.store.undo()
  await ui.store.undo()
  assert.deepEqual(layout(ui.store.groups), original)
  assert.deepEqual(layout(ui.remote), original)
  assert.equal(ui.store.undoCount, 0)
})

test('actual navigation store invalidates stale undo and reloads the newer server state', async () => {
  const ui = harness()
  await ui.store.load()
  await ui.store.deleteItem(11)
  ui.externalChange()
  await assert.rejects(ui.store.undo(), error => !!error && typeof error === 'object' && 'code' in error && error.code === 1409)
  assert.equal(ui.store.undoCount, 0)
  assert.equal(ui.store.needsVerification, false)
  assert.equal(ui.store.revision, ui.revision)
  assert.deepEqual(plain(ui.store.groups), ui.remote)
  assert.equal(ui.writes.filter(write => write.kind === 'restore').length, 1)
})

test('actual navigation store verifies a lost restore acknowledgement using reads without replay', async () => {
  const ui = harness()
  await ui.store.load()
  await ui.store.deleteItem(11)
  ui.loseNextRestoreAcknowledgement()
  await assert.rejects(ui.store.undo(), RequestError)
  assert.equal(ui.store.undoCount, 0)
  assert.equal(ui.store.needsVerification, true)
  assert.equal(ui.store.canUndo, false)
  await ui.store.verifyMove()
  assert.equal(ui.store.needsVerification, false)
  assert.deepEqual(layout(ui.store.groups), layout(ui.remote))
  assert.equal(ui.writes.filter(write => write.kind === 'restore').length, 1)
})

test('actual navigation store preserves an acknowledged deletion and its inverse when the read fails', async () => {
  const ui = harness()
  await ui.store.load()
  ui.failNextRead()
  await ui.store.deleteItem(11)
  assert.equal(ui.store.undoCount, 1)
  assert.equal(ui.store.revision, ui.revision)
  assert.deepEqual(layout(ui.store.groups), layout(ui.remote))
  assert.match(ui.store.error, /Follow-up read unavailable/)
  if (ui.store.needsVerification) await ui.store.verifyMove()
  assert.equal(ui.store.undoCount, 1, 'verifying an acknowledged write must retain its inverse')
  await ui.store.undo()
  assert.deepEqual(layout(ui.store.groups), layout(ui.remote))
})

test('actual store restores the deleted item at its original index after a sibling move and undo', async () => {
  const ui = harness()
  await ui.store.load()
  const original = layout(ui.remote)
  await ui.store.deleteItem(12)
  await ui.store.move(11, 1, 1)
  await ui.store.undo()
  await ui.store.undo()
  assert.deepEqual(layout(ui.store.groups), original)
  const restored = ui.writes.find(write => write.kind === 'restore')!.payload as TrashRef
  assert.equal(restored.targetGroupId, 1)
  assert.equal(restored.targetIndex, 1)
})

test('acknowledged sorts and restores either apply locally or block further writes when their read fails', async () => {
  const ui = harness()
  await ui.store.load()
  ui.failNextRead()
  await ui.store.sortGroups([{ id: 3, sort: 0 }, { id: 2, sort: 1 }, { id: 1, sort: 2 }])
  assert.equal(ui.store.revision, ui.revision)
  assert.ok(ui.store.needsVerification || JSON.stringify(layout(ui.store.groups)) === JSON.stringify(layout(ui.remote)), 'an acknowledged sort must be reflected locally or require verification')
  if (ui.store.needsVerification) await ui.store.verifyMove()
  await ui.store.deleteItem(11)
  ui.failNextRead()
  await ui.store.undo()
  assert.equal(ui.store.revision, ui.revision)
  assert.ok(ui.store.needsVerification || JSON.stringify(layout(ui.store.groups)) === JSON.stringify(layout(ui.remote)), 'an acknowledged restore must be reflected locally or require verification')
  assert.equal(ui.writes.filter(write => write.kind === 'restore').length, 1)
})
