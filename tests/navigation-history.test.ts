import assert from 'node:assert/strict'
import test from 'node:test'
import { createNavigationHistory } from '../src/utils/navigationHistory.ts'
import type { NavigationUndoRecord } from '../src/utils/navigationHistory.ts'

test('undoing our own writes rebases the next inverse without losing earlier history', () => {
  let top: NavigationUndoRecord | undefined
  let count = 0
  const history = createNavigationHistory((record, length) => { top = record; count = length })
  history.record({ kind: 'delete', entry: { kind: 'item', id: 4, deletionId: 'first-delete' }, title: 'NAS' }, 10)
  history.record({ kind: 'move', itemId: 8, targetGroupId: 2, targetIndex: 0, title: 'Media' }, 12)
  history.completeUndo(14)
  assert.equal(count, 1)
  assert.equal(top?.action.kind, 'delete')
  assert.equal(top?.revision, 14)
  history.reconcile(14)
  assert.equal(count, 1)
  history.completeUndo(15)
  assert.equal(count, 0)
  assert.equal(top, undefined)
})

test('external edits invalidate undo rather than overwriting newer navigation', () => {
  let count = 0
  const history = createNavigationHistory((_record, length) => { count = length })
  history.record({ kind: 'sort', order: [{ id: 1, sort: 2 }, { id: 2, sort: 1 }] }, 5)
  history.reconcile(6)
  assert.equal(count, 0)
})

test('history keeps an isolated bounded copy of inverse actions', () => {
  let top: NavigationUndoRecord | undefined
  let count = 0
  const history = createNavigationHistory((record, length) => { top = record; count = length }, 2)
  const order = [{ id: 1, sort: 1 }]
  history.record({ kind: 'sort', order }, 1)
  order[0].sort = 99
  assert.deepEqual(top?.action, { kind: 'sort', order: [{ id: 1, sort: 1 }] })
  history.record({ kind: 'sort', order }, 2)
  history.record({ kind: 'sort', order }, 3)
  assert.equal(count, 2)
  history.completeUndo(4)
  history.completeUndo(5)
  assert.equal(count, 0)
})
