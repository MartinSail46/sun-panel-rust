import assert from 'node:assert/strict'
import test from 'node:test'
import { confirmDialog, currentConfirmation, resolveConfirmation } from '../src/lib/feedback.ts'

test('confirmation requests remain ordered and stale close events cannot dismiss the next request', async () => {
  const first = confirmDialog({ title: 'Delete first', description: 'First item' })
  const firstId = currentConfirmation.value!.id
  const second = confirmDialog({ title: 'Delete second', description: 'Second item' })
  assert.equal(currentConfirmation.value!.id, firstId)
  resolveConfirmation(firstId, true)
  assert.equal(await first, true)
  const secondId = currentConfirmation.value!.id
  assert.notEqual(firstId, secondId)
  resolveConfirmation(firstId, false)
  assert.equal(currentConfirmation.value!.id, secondId)
  resolveConfirmation(secondId, false)
  assert.equal(await second, false)
  assert.equal(currentConfirmation.value, undefined)
})
