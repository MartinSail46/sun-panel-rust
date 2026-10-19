import assert from 'node:assert/strict'
import test from 'node:test'
import { cloneDraft, filterNavigation, moved, searchUrl, selectionIndex } from '../src/store/modules/navigation/helpers.ts'

test('editing a cloned group or nested icon cannot change its original', () => {
  const original = { id: 4, title: 'APP', icon: { src: '/uploads/a.png' } }
  const draft = cloneDraft(original)
  draft.title = 'Cancelled'
  draft.icon.src = '/uploads/b.png'
  assert.deepEqual(original, { id: 4, title: 'APP', icon: { src: '/uploads/a.png' } })
})
test('application filtering keeps group and item identity without changing stored arrays', () => {
  const groups = [{ id: 8, title: 'NAS', items: [{ id: 9, title: 'Media', url: 'http://nas.local', description: 'Movies' }, { id: 10, title: 'Tools', url: 'http://tool.local' }] }]
  const result = filterNavigation(groups, ' MOVIES ')
  assert.equal(result.length, 1)
  assert.equal(result[0].id, 8)
  assert.equal(result[0].items[0].id, 9)
  assert.equal(groups[0].items.length, 2)
  assert.deepEqual(filterNavigation(groups, 'missing'), [])
})
test('keyboard reordering is bounded and does not mutate its input', () => {
  const input = [3, 1, 2]
  assert.deepEqual(moved(input, 1, -1), [1, 3, 2])
  assert.deepEqual(input, [3, 1, 2])
  assert.equal(moved(input, 0, -1), input)
  assert.equal(moved(input, 2, 1), input)
})
test('search selection wraps safely and empty results have no selection', () => {
  assert.equal(selectionIndex(0, -1, 3), 2)
  assert.equal(selectionIndex(2, 1, 3), 0)
  assert.equal(selectionIndex(0, 1, 0), -1)
})
test('web search encodes user input and only replaces its placeholder', () => {
  assert.equal(searchUrl('https://search.test?q=%s', ' a&b 中 '), 'https://search.test?q=a%26b%20%E4%B8%AD')
})
