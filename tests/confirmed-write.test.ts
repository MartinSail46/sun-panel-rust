import assert from 'node:assert/strict'
import test from 'node:test'
import { createConfirmedWriter } from '../src/utils/confirmedWrite.ts'
import { RequestError } from '../src/utils/requestLifetime.ts'

test('a timed-out committed write is verified as saved without replaying it', async () => {
  let calls = 0
  const write = createConfirmedWriter({
    read: async () => ({ value: { color: 'blue', nested: { b: 2, a: 1 } }, revision: 8 }),
    write: async () => { calls++; throw new RequestError('timeout', 'write') },
  })
  assert.equal(await write({ nested: { a: 1, b: 2 }, color: 'blue' }, 7), 8)
  assert.equal(calls, 1)
})

test('unknown uncommitted writes stop; a later retry must read before CAS again', async () => {
  const events: string[] = []
  const value = { title: 'old' }
  let calls = 0
  const write = createConfirmedWriter({
    read: async () => { events.push('read'); return { value, revision: 1 } },
    write: async () => { events.push('write'); if (++calls === 1) throw new RequestError('network', 'write'); return 2 },
  })
  await assert.rejects(write({ title: 'new' }, 1), { outcomeUnknown: true })
  assert.deepEqual(events, ['write', 'read'])
  assert.equal(await write({ title: 'new' }, 1), 2)
  assert.deepEqual(events, ['write', 'read', 'read', 'write'])
})

test('failed verification prevents a later write from being sent', async () => {
  let calls = 0
  const write = createConfirmedWriter({
    read: async (): Promise<{ value: string; revision: number }> => { throw new Error('offline') },
    write: async () => { calls++; throw new RequestError('timeout', 'write') },
  })
  await assert.rejects(write('new', 1), { outcomeUnknown: true })
  await assert.rejects(write('new', 1), { outcomeUnknown: true })
  assert.equal(calls, 1)
})

test('a revision changed during an uncertain write is surfaced as conflict, never overwritten', async () => {
  let calls = 0
  const write = createConfirmedWriter({
    read: async () => ({ value: 'other client', revision: 3 }),
    write: async () => { calls++; throw new RequestError('cancelled', 'write') },
  })
  await assert.rejects(write('my draft', 1), { outcomeUnknown: true })
  await assert.rejects(write('my draft', 1), error => (error as { code: number }).code === 1409)
  assert.equal(calls, 1)
})
