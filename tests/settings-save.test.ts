import assert from 'node:assert/strict'
import { setImmediate } from 'node:timers/promises'
import { test } from 'node:test'
import { createSaveQueue } from '../src/utils/settingsSave.ts'

function deferred() {
  let resolve!: () => void
  let reject!: (error: Error) => void
  const promise = new Promise<void>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

test('debounces rapid edits and snapshots values before callers mutate them', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const written: number[] = []
  const queue = createSaveQueue(async value => { written.push(value.count) }, 700)
  const value = { count: 1 }
  queue.enqueue(value)
  context.mock.timers.tick(500)
  value.count = 2
  queue.enqueue(value)
  context.mock.timers.tick(500)
  assert.deepEqual(written, [])
  value.count = 99
  context.mock.timers.tick(200)
  await setImmediate()
  assert.deepEqual(written, [2])
})

test('serializes slow writes and merges edits into the latest queued value', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const requests: Array<{ count: number, done: ReturnType<typeof deferred> }> = []
  let inFlight = 0
  let maximum = 0
  const queue = createSaveQueue(async (value: { count: number }) => {
    inFlight++
    maximum = Math.max(maximum, inFlight)
    const done = deferred()
    requests.push({ count: value.count, done })
    await done.promise
    inFlight--
  }, 700)

  queue.enqueue({ count: 1 })
  context.mock.timers.tick(700)
  queue.enqueue({ count: 2 })
  context.mock.timers.tick(700)
  queue.enqueue({ count: 3 })
  context.mock.timers.tick(700)
  assert.deepEqual(requests.map(request => request.count), [1])
  const flushed = queue.flush()
  requests[0].done.resolve()
  await setImmediate()
  assert.deepEqual(requests.map(request => request.count), [1, 3])

  queue.enqueue({ count: 4 })
  queue.enqueue({ count: 5 })
  requests[1].done.resolve()
  await setImmediate()
  assert.deepEqual(requests.map(request => request.count), [1, 3, 5])
  requests[2].done.resolve()
  assert.equal(await flushed, true)
  assert.equal(maximum, 1)
  assert.equal(inFlight, 0)
})

test('keeps the newest failed value for explicit retry without an automatic retry loop', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const first = deferred()
  const written: number[] = []
  let status = ''
  let error: string | undefined
  const queue = createSaveQueue(async (value: { count: number }) => {
    written.push(value.count)
    if (written.length === 1)
      await first.promise
  })
  queue.subscribe((state) => {
    status = state.status
    error = state.error
  })

  queue.enqueue({ count: 1 })
  const failed = queue.flush()
  queue.enqueue({ count: 2 })
  first.reject(new Error('offline'))
  assert.equal(await failed, false)
  assert.equal(status, 'error')
  assert.equal(error, 'offline')
  context.mock.timers.tick(60_000)
  await setImmediate()
  assert.deepEqual(written, [1])
  assert.equal(await queue.flush(), true)
  assert.deepEqual(written, [1, 2])
  assert.equal(status, 'saved')
})

test('manual flush bypasses debounce, deduplicates reset/watch values and survives unsubscribing', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const done = deferred()
  const written: string[] = []
  const states: string[] = []
  const queue = createSaveQueue(async (value: { text: string }) => {
    written.push(value.text)
    await done.promise
  })
  const unsubscribe = queue.subscribe(state => states.push(state.status))
  queue.enqueue({ text: 'reset' })
  queue.enqueue({ text: 'reset' })
  const firstFlush = queue.flush()
  const secondFlush = queue.flush()
  assert.deepEqual(written, ['reset'])
  unsubscribe()
  done.resolve()
  assert.equal(await firstFlush, true)
  assert.equal(await secondFlush, true)
  context.mock.timers.tick(1000)
  await setImmediate()
  queue.enqueue({ text: 'reset' })
  assert.equal(await queue.flush(), true)
  assert.deepEqual(written, ['reset'])
  assert.deepEqual(states, ['idle', 'pending', 'saving'])
})

test('reverting to the in-flight value discards an obsolete queued edit', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const done = deferred()
  const written: number[] = []
  const queue = createSaveQueue(async (value: { count: number }) => {
    written.push(value.count)
    await done.promise
  })
  queue.enqueue({ count: 1 })
  const flushed = queue.flush()
  queue.enqueue({ count: 2 })
  queue.enqueue({ count: 1 })
  done.resolve()
  assert.equal(await flushed, true)
  context.mock.timers.tick(1000)
  await setImmediate()
  assert.deepEqual(written, [1])
})

