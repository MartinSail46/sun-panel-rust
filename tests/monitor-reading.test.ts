/* eslint test/no-import-node-test: off -- Uses Node's built-in test runner. */
import assert from 'node:assert/strict'
import test from 'node:test'
import { acceptMonitorReading, createMonitorReading, formatMonitorCapacity, monitorReadingIsStale, rejectMonitorReading } from '../src/utils/monitorReading.ts'

test('a first monitor failure remains unknown instead of appearing as zero', () => {
  const reading = createMonitorReading<{ used: number, total: number }>()
  assert.equal(reading.loading, true)
  rejectMonitorReading(reading, new Error('Disk path unavailable'))
  assert.equal(reading.value, null)
  assert.equal(reading.updatedAt, null)
  assert.equal(reading.loading, false)
  assert.equal(reading.error, 'Disk path unavailable')
})

test('a disk failure preserves its last measurement and timestamp but marks it stale', () => {
  const reading = createMonitorReading<{ used: number, total: number }>()
  acceptMonitorReading(reading, { used: 20, total: 100 }, undefined, 1000)
  assert.equal(monitorReadingIsStale(reading, 5000, 2000), false)
  rejectMonitorReading(reading, 'disconnected')
  assert.deepEqual(reading.value, { used: 20, total: 100 })
  assert.equal(reading.updatedAt, 1000)
  assert.equal(monitorReadingIsStale(reading, 5000, 2000), true)
  acceptMonitorReading(reading, { used: 25, total: 100 }, undefined, 3000)
  assert.equal(reading.error, null)
  assert.equal(reading.updatedAt, 3000)
  assert.equal(monitorReadingIsStale(reading, 5000, 19000), true)
})

test('the server measurement timestamp is preserved instead of extending a cached reading', () => {
  const reading = createMonitorReading<number>()
  acceptMonitorReading(reading, 20, '2026-10-19T00:00:00Z', Date.parse('2026-10-19T00:01:00Z'))
  assert.equal(reading.updatedAt, Date.parse('2026-10-19T00:00:00Z'))
  assert.equal(monitorReadingIsStale(reading, 5000, Date.parse('2026-10-19T00:01:00Z')), true)
})


test('RAM and disk capacities always show used out of total, without substituting free space', () => {
  const measurement = { used: 20, total: 100, free: 80 }
  assert.equal(formatMonitorCapacity(measurement, value => `${value} GiB`), '20 GiB / 100 GiB')
  assert.equal(formatMonitorCapacity(null, String), '—')
})
