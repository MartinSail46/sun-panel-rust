import assert from 'node:assert/strict'
import { getEventListeners } from 'node:events'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { test } from 'node:test'
import axios from 'axios'
import { isRequestCancelled, isUncertainWrite, RequestError, requestFailureMessage, REQUEST_TIMEOUTS, runRequestLifecycle } from '../src/utils/requestLifetime.ts'
import { createForegroundScheduler } from '../src/utils/foregroundScheduler.ts'

test('separate read, write and transfer budgets remain bounded', () => {
  assert.deepEqual(REQUEST_TIMEOUTS, { read: 10_000, write: 15_000, import: 35_000, upload: 35_000 })
})

test('an absolute deadline settles an adapter that ignores AbortSignal without replaying writes', async () => {
  let before = 0
  let after = 0
  let calls = 0
  let adapterSignal: AbortSignal | undefined
  const owner = new AbortController()
  await assert.rejects(runRequestLifecycle((signal) => {
    calls++
    adapterSignal = signal
    return new Promise(() => {})
  }, { operation: 'write', timeoutMs: 20, signal: owner.signal, beforeRequest: () => before++, afterRequest: () => after++ }), (error: unknown) => {
    assert.ok(error instanceof RequestError)
    assert.equal(error.kind, 'timeout')
    assert.ok(isUncertainWrite(error))
    assert.equal(requestFailureMessage(error), '请求超时，请稍后重试')
    return true
  })
  assert.equal(calls, 1)
  assert.equal(before, 1)
  assert.equal(after, 1)
  assert.equal(adapterSignal?.aborted, true)
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
})

test('a pre-cancelled owner starts no network work and finalizes once', async () => {
  const owner = new AbortController()
  owner.abort()
  let after = 0
  await assert.rejects(runRequestLifecycle(async () => assert.fail('cancelled work must never be dispatched'), {
    signal: owner.signal,
    afterRequest: () => after++,
  }), isRequestCancelled)
  assert.equal(after, 1)
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
})

test('manual cancellation wins over a pending deadline and never produces an error toast', async () => {
  const owner = new AbortController()
  let after = 0
  const request = runRequestLifecycle(() => new Promise(() => {}), {
    operation: 'write', signal: owner.signal, timeoutMs: 100, afterRequest: () => after++,
  })
  owner.abort()
  await assert.rejects(request, (error: unknown) => {
    assert.ok(isRequestCancelled(error))
    assert.ok(isUncertainWrite(error), 'client cancellation cannot claim that a write was rolled back')
    assert.equal(requestFailureMessage(error), undefined)
    return true
  })
  assert.equal(after, 1)
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
})

test('success clears the deadline and external listener without aborting completed work', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const owner = new AbortController()
  let signal: AbortSignal | undefined
  let after = 0
  const result = await runRequestLifecycle(async (current) => {
    signal = current
    return { code: 0 }
  }, { signal: owner.signal, timeoutMs: 50, afterRequest: () => after++ })
  assert.deepEqual(result, { code: 0 })
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
  context.mock.timers.tick(10_000)
  owner.abort()
  assert.equal(signal?.aborted, false)
  assert.equal(after, 1)
})

test('business revision conflicts retain the full response and finalize exactly once', async () => {
  const conflict = { code: 1409, currentRevision: 27, data: { revision: 27 }, msg: 'Changed elsewhere' }
  const owner = new AbortController()
  let after = 0
  await assert.rejects(runRequestLifecycle(async () => conflict, {
    operation: 'write', signal: owner.signal, afterRequest: () => after++,
  }, (response) => { throw response }), (error: unknown) => error === conflict)
  assert.equal(after, 1)
  assert.equal(requestFailureMessage(conflict), undefined)
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
})

test('connection failures and timeouts are distinct and silent background requests stay silent', async () => {
  await assert.rejects(runRequestLifecycle(async () => { throw new Error('ECONNRESET') }, { operation: 'read' }), (error: unknown) => {
    assert.ok(error instanceof RequestError)
    assert.equal(error.kind, 'network')
    assert.equal(error.outcomeUnknown, false)
    assert.equal(requestFailureMessage(error), '网络错误，请稍后重试')
    assert.equal(requestFailureMessage(error, true), undefined)
    return true
  })
})

test('deadline aborts a real HTTP response that flushes headers then stalls the body', async (context) => {
  let requestCount = 0
  let received!: () => void
  const arrived = new Promise<void>((resolve) => { received = resolve })
  const server = createServer((_, response) => {
    requestCount++
    response.writeHead(200, { 'Content-Type': 'application/json' })
    response.flushHeaders()
    response.write('{"data":')
    received()
  })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  context.after(() => {
    server.closeAllConnections()
    server.close()
  })
  const address = server.address() as AddressInfo
  const started = Date.now()
  const outcome = assert.rejects(runRequestLifecycle(signal => axios.get(`http://127.0.0.1:${address.port}`, { signal, proxy: false }), {
    operation: 'read', timeoutMs: 250,
  }), (error: unknown) => error instanceof RequestError && error.kind === 'timeout')
  await arrived
  await outcome
  assert.equal(requestCount, 1)
  assert.ok(Date.now() - started < 2000, 'headers do not turn the wall-clock deadline into an endless response wait')
})

test('owner abort interrupts a real HTTP connection and releases its listener', async (context) => {
  let received!: () => void
  const arrived = new Promise<void>((resolve) => { received = resolve })
  const server = createServer(() => { received() })
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve))
  context.after(() => {
    server.closeAllConnections()
    server.close()
  })
  const address = server.address() as AddressInfo
  const owner = new AbortController()
  let after = 0
  const outcome = assert.rejects(runRequestLifecycle(signal => axios.get(`http://127.0.0.1:${address.port}`, { signal, proxy: false }), {
    signal: owner.signal, timeoutMs: 2000, afterRequest: () => after++,
  }), isRequestCancelled)
  await arrived
  owner.abort()
  await outcome
  assert.equal(after, 1)
  assert.equal(getEventListeners(owner.signal, 'abort').length, 0)
})

test('hidden monitors and removed subscribers abort in-flight work without reporting a polling failure', async () => {
  let visible = true
  let failures = 0
  const signals: AbortSignal[] = []
  let started!: () => void
  let arrived = new Promise<void>((resolve) => { started = resolve })
  const scheduler = createForegroundScheduler({
    isVisible: () => visible,
    interval: () => 5000,
    onError: () => failures++,
    run: signal => new Promise<void>((_, reject) => {
      signals.push(signal)
      signal.addEventListener('abort', () => reject(signal.reason), { once: true })
      started()
    }),
  })
  scheduler.start()
  await arrived
  visible = false
  scheduler.visibilityChanged()
  assert.equal(signals[0].aborted, true)
  // Let the cancelled run's finally block retire before resuming.
  await new Promise(resolve => setTimeout(resolve, 0))
  arrived = new Promise<void>((resolve) => { started = resolve })
  visible = true
  scheduler.visibilityChanged()
  await arrived
  assert.equal(signals.length, 2)
  assert.equal(signals[1].aborted, false)
  scheduler.stop()
  assert.equal(signals[1].aborted, true)
  await new Promise(resolve => setTimeout(resolve, 0))
  assert.equal(failures, 0)
})
