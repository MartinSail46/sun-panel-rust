import assert from 'node:assert/strict'
import { test } from 'node:test'
import { persistMonitorOrder } from '../src/components/deskModule/SystemMonitor/order.ts'
import type { MonitorData } from '../src/components/deskModule/SystemMonitor/typings.ts'

const before = [{ monitorType: 'cpu' }, { monitorType: 'memory' }] as MonitorData[]
const ordered = [...before].reverse()

test('a monitor drop persists its entire new order once without requiring a separate sort mode', async () => {
  const writes: unknown[] = []
  const result = await persistMonitorOrder(before, ordered, {
    async save(value) { writes.push(value); return { code: 0 } },
    async read() { throw new Error('No read needed after a confirmed write') },
  })
  assert.deepEqual(writes, [ordered])
  assert.deepEqual(result.value, ordered)
  assert.equal(result.confirmed, true)
  assert.equal(result.needsReload, false)
})

test('dropping a monitor in its original position does not write', async () => {
  const result = await persistMonitorOrder(before, before, {
    async save() { throw new Error('No mutation needed') },
    async read() { throw new Error('No read needed') },
  })
  assert.deepEqual(result.value, before)
  assert.equal(result.confirmed, true)
})

test('an uncertain committed order is acknowledged by a read without replaying its write', async () => {
  let writes = 0
  let reads = 0
  const result = await persistMonitorOrder(before, ordered, {
    async save() { writes++; throw new Error('Connection lost after commit') },
    async read() { reads++; return ordered },
  })
  assert.equal(writes, 1)
  assert.equal(reads, 1)
  assert.equal(result.confirmed, true)
  assert.equal(result.needsReload, false)
  assert.deepEqual(result.value, ordered)
})

test('readback recognizes a committed order even when JSON object keys were reordered', async () => {
  const desired = [{ monitorType: 'disk', extendParam: { path: '/', color: '#fff' } }, ...ordered] as MonitorData[]
  const actual = [{ extendParam: { color: '#fff', path: '/' }, monitorType: 'disk' }, ...ordered] as MonitorData[]
  const result = await persistMonitorOrder(before, desired, {
    async save() { throw new Error('Connection lost after commit') },
    async read() { return actual },
  })
  assert.equal(result.confirmed, true)
  assert.deepEqual(result.value, actual)
})

test('a failed order shows the actual server order and keeps editing available only after a successful read', async () => {
  const result = await persistMonitorOrder(before, ordered, {
    async save() { return { code: 1409, msg: 'Configuration changed' } },
    async read() { return before },
  })
  assert.equal(result.confirmed, false)
  assert.equal(result.needsReload, false)
  assert.deepEqual(result.value, before)
})

test('a failed verification retains the last confirmed order and requires an explicit reload', async () => {
  let writes = 0
  const result = await persistMonitorOrder(before, ordered, {
    async save() { writes++; throw new Error('Unknown outcome') },
    async read() { throw new Error('Offline') },
  })
  assert.equal(writes, 1)
  assert.equal(result.confirmed, false)
  assert.equal(result.needsReload, true)
  assert.deepEqual(result.value, before)
})
