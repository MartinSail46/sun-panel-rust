/* eslint test/no-import-node-test: off -- Runs with Node's built-in test runner without adding a test dependency. */
import assert from 'node:assert/strict'
import test from 'node:test'
import { createForegroundScheduler, nextClockBoundary } from '../src/utils/foregroundScheduler.ts'
import type { SchedulerTimer } from '../src/utils/foregroundScheduler.ts'

async function settle() {
  for (let index = 0; index < 8; index++)
    await Promise.resolve()
}

function fakeTimer(start = 0) {
  let time = start
  let serial = 0
  const scheduled = new Map<number, { at: number, callback: () => void }>()
  const timer: SchedulerTimer = {
    now: () => time,
    setTimeout(callback, delay) {
      const id = ++serial
      scheduled.set(id, { at: time + delay, callback })
      return id
    },
    clearTimeout(handle) {
      scheduled.delete(handle as number)
    },
  }
  return {
    timer,
    get pending() { return scheduled.size },
    async advance(milliseconds: number) {
      const target = time + milliseconds
      let iterations = 0
      while (true) {
        const next = Array.from(scheduled).sort((a, b) => a[1].at - b[1].at)[0]
        if (!next || next[1].at > target)
          break
        assert.ok(iterations++ < 100, 'scheduler must not spin without a delay')
        time = next[1].at
        scheduled.delete(next[0])
        next[1].callback()
        await settle()
      }
      time = target
      await settle()
    },
  }
}

test('monitor scheduling waits for the request, then waits a full interval', async () => {
  const fake = fakeTimer()
  let calls = 0
  let finish = () => {}
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => true,
    run: () => {
      calls++
      return new Promise<void>((resolve) => { finish = resolve })
    },
  })
  scheduler.start()
  await fake.advance(20000)
  assert.equal(calls, 1, 'a slow request cannot overlap the next poll')
  assert.equal(fake.pending, 0)
  finish()
  await settle()
  await fake.advance(4999)
  assert.equal(calls, 1)
  await fake.advance(1)
  assert.equal(calls, 2)
  scheduler.stop()
  finish()
  await settle()
  assert.equal(fake.pending, 0)
})

test('hidden documents cancel pending work and resume immediately', async () => {
  const fake = fakeTimer()
  let visible = true
  let calls = 0
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => visible,
    run: () => { calls++ },
  })
  scheduler.start()
  await fake.advance(0)
  assert.equal(calls, 1)
  visible = false
  scheduler.visibilityChanged()
  assert.equal(fake.pending, 0)
  await fake.advance(60000)
  assert.equal(calls, 1)
  visible = true
  scheduler.visibilityChanged()
  await fake.advance(0)
  assert.equal(calls, 2)
  scheduler.stop()
})

test('visibility and explicit refresh during a request coalesce into one next run', async () => {
  const fake = fakeTimer()
  let visible = true
  let calls = 0
  let finish = () => {}
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => visible,
    run: () => {
      calls++
      return new Promise<void>((resolve) => { finish = resolve })
    },
  })
  scheduler.start()
  await fake.advance(0)
  visible = false
  scheduler.visibilityChanged()
  visible = true
  scheduler.visibilityChanged()
  scheduler.refresh()
  scheduler.refresh()
  await fake.advance(30000)
  assert.equal(calls, 1)
  finish()
  await settle()
  assert.equal(fake.pending, 1)
  await fake.advance(0)
  assert.equal(calls, 2)
  scheduler.stop()
  finish()
  await settle()
})

test('repeated failures back off to one minute and success restores normal polling', async () => {
  const fake = fakeTimer()
  let calls = 0
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => true,
    run: () => {
      calls++
      if (calls <= 4)
        throw new Error('temporarily offline')
    },
  })
  scheduler.start()
  await fake.advance(0)
  assert.equal(calls, 1)
  await fake.advance(9999)
  assert.equal(calls, 1)
  await fake.advance(1)
  assert.equal(calls, 2)
  await fake.advance(20000)
  assert.equal(calls, 3)
  await fake.advance(40000)
  assert.equal(calls, 4)
  await fake.advance(59999)
  assert.equal(calls, 4)
  await fake.advance(1)
  assert.equal(calls, 5)
  await fake.advance(5000)
  assert.equal(calls, 6)
  scheduler.stop()
})

test('unmount and remount during a request still keep only one request in flight', async () => {
  const fake = fakeTimer()
  let calls = 0
  let finish = () => {}
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => true,
    run: () => {
      calls++
      return new Promise<void>((resolve) => { finish = resolve })
    },
  })
  scheduler.start()
  await fake.advance(0)
  scheduler.stop()
  scheduler.start()
  await fake.advance(10000)
  assert.equal(calls, 1)
  finish()
  await settle()
  await fake.advance(0)
  assert.equal(calls, 2)
  scheduler.stop()
  finish()
  await settle()
  assert.equal(fake.pending, 0)
})

test('starting while hidden stays idle and stopping removes all timers', async () => {
  const fake = fakeTimer()
  let visible = false
  let calls = 0
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => 5000,
    isVisible: () => visible,
    run: () => { calls++ },
  })
  scheduler.start()
  assert.equal(fake.pending, 0)
  await fake.advance(60000)
  assert.equal(calls, 0)
  visible = true
  scheduler.visibilityChanged()
  scheduler.stop()
  await fake.advance(60000)
  assert.equal(calls, 0)
  assert.equal(fake.pending, 0)
})

test('clock delays align to second and minute boundaries without interval drift', () => {
  assert.equal(nextClockBoundary(123456, false), 544)
  assert.equal(nextClockBoundary(123456, true), 56544)
  assert.equal(nextClockBoundary(180000, true), 60000)
  assert.equal(nextClockBoundary(180000, false), 1000)
})

test('a clock with hidden seconds updates once per minute and realigns after visibility', async () => {
  const fake = fakeTimer(123456)
  let visible = true
  let calls = 0
  let hideSecond = true
  const scheduler = createForegroundScheduler({
    timer: fake.timer,
    interval: () => hideSecond ? 60000 : 1000,
    successDelay: now => nextClockBoundary(now, hideSecond),
    isVisible: () => visible,
    run: () => { calls++ },
  })
  scheduler.start()
  await fake.advance(56543)
  assert.equal(calls, 1)
  await fake.advance(1)
  assert.equal(calls, 2)
  visible = false
  scheduler.visibilityChanged()
  await fake.advance(90000)
  assert.equal(calls, 2)
  visible = true
  scheduler.visibilityChanged()
  await fake.advance(0)
  assert.equal(calls, 3)
  hideSecond = false
  scheduler.refresh()
  await fake.advance(0)
  assert.equal(calls, 4)
  await fake.advance(1000)
  assert.equal(calls, 5)
  scheduler.stop()
})
