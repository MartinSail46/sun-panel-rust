import assert from 'node:assert/strict'
import { setImmediate } from 'node:timers/promises'
import test from 'node:test'
import { createPanelSync } from '../src/utils/panelSync.ts'

interface Config { title: string; color: string; wallpaper?: string }
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}
function remote(initial: Config = { title: 'Original', color: 'black' }) {
  let value = clone(initial)
  let revision = 1
  const writes: Array<{ value: Config, revision: number }> = []
  return {
    writes,
    read: async () => ({ value: clone(value), revision }),
    async write(next: Config, expectedRevision: number) {
      writes.push({ value: clone(next), revision: expectedRevision })
      if (expectedRevision !== revision)
        throw Object.assign(new Error('conflict'), { code: 1409 })
      value = clone(next)
      return ++revision
    },
    replace(next: Config) { value = clone(next); revision++ },
    get value() { return clone(value) },
  }
}
function page(server: ReturnType<typeof remote>) {
  let view: Config = { title: 'Original', color: 'black' }
  const sync = createPanelSync(view, { read: server.read, write: server.write, apply: value => { view = clone(value) } })
  return { sync, get view() { return clone(view) } }
}

test('edits made during initial load preserve intent while using unrelated server fields', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const read = deferred<{ value: Config, revision: number }>()
  const writes: Config[] = []
  let view: Config = { title: 'Default', color: 'black' }
  const sync = createPanelSync(view, {
    read: () => read.promise,
    write: async (value, revision) => { assert.equal(revision, 7); writes.push(clone(value)); return 8 },
    apply: value => { view = clone(value) },
  })
  const loading = sync.load()
  sync.observe({ title: 'Local edit', color: 'black' })
  const saved = sync.flush()
  read.resolve({ value: { title: 'Remote title', color: 'blue', wallpaper: '/server.png' }, revision: 7 })
  await loading
  assert.equal(await saved, true)
  assert.deepEqual(writes, [{ title: 'Local edit', color: 'blue', wallpaper: '/server.png' }])
  assert.deepEqual(view, writes[0])
})

test('two pages changing different fields rebase without losing remote data', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote()
  const first = page(server)
  const second = page(server)
  await Promise.all([first.sync.load(), second.sync.load()])
  first.sync.observe({ ...first.view, title: 'From first page' })
  assert.equal(await first.sync.flush(), true)
  second.sync.observe({ ...second.view, color: 'green' })
  assert.equal(await second.sync.flush(), true)
  assert.deepEqual(server.value, { title: 'From first page', color: 'green' })
  assert.deepEqual(second.view, server.value)
  assert.equal(server.writes.length, 3)
})

test('same-field conflicts retain the local draft until explicit reload discards it', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote()
  const first = page(server)
  const second = page(server)
  await Promise.all([first.sync.load(), second.sync.load()])
  first.sync.observe({ ...first.view, title: 'Remote new title' })
  await first.sync.flush()
  const draft = { ...second.view, title: 'Unsaved local title' }
  second.sync.observe(draft)
  let status = ''
  let message = ''
  second.sync.subscribe(value => { status = value.status; message = value.error || '' })
  assert.equal(await second.sync.flush(), false)
  assert.equal(status, 'error')
  assert.match(message, /同一设置/)
  assert.equal(server.value.title, 'Remote new title')
  assert.equal(await second.sync.reload(), true)
  assert.deepEqual(second.view, server.value)
  assert.equal(await second.sync.flush(), true)
  assert.equal(server.writes.length, 2)
})

test('network failures keep the latest pending draft for a successful explicit retry', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote()
  let offline = true
  let view = { title: 'Original', color: 'black' }
  const sync = createPanelSync(view, {
    read: server.read,
    write: async (value, revision) => {
      if (offline)
        throw new Error('offline')
      return server.write(value, revision)
    },
    apply: value => { view = clone(value) },
  })
  await sync.load()
  sync.observe({ ...view, title: 'First attempt' })
  assert.equal(await sync.flush(), false)
  sync.observe({ ...view, title: 'Latest draft', color: 'orange' })
  offline = false
  assert.equal(await sync.flush(), true)
  assert.deepEqual(server.value, { title: 'Latest draft', color: 'orange' })
  assert.deepEqual(view, server.value)
})

test('slow writes stay serialized and later local fields survive completion of an older request', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote()
  const firstWrite = deferred<void>()
  let calls = 0
  let active = 0
  let maximum = 0
  let view: Config = server.value
  const sync = createPanelSync(view, {
    read: server.read,
    async write(value, revision) {
      active++
      maximum = Math.max(maximum, active)
      if (++calls === 1)
        await firstWrite.promise
      const result = await server.write(value, revision)
      active--
      return result
    },
    apply: value => { view = clone(value) },
  })
  await sync.load()
  sync.observe({ ...view, title: 'New title' })
  const saved = sync.flush()
  await setImmediate()
  sync.observe({ title: 'New title', color: 'green' })
  sync.observe({ title: 'Newest title', color: 'orange' })
  firstWrite.resolve()
  assert.equal(await saved, true)
  assert.equal(maximum, 1)
  assert.deepEqual(server.value, { title: 'Newest title', color: 'orange' })
  assert.deepEqual(view, server.value)
})

test('deleting an optional field is a local intent and preserves other remote values', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote({ title: 'Original', color: 'black', wallpaper: '/old.png' })
  const current = page(server)
  await current.sync.load()
  current.sync.observe({ title: 'Original', color: 'black' })
  server.replace({ title: 'Other page', color: 'black', wallpaper: '/old.png' })
  assert.equal(await current.sync.flush(), true)
  assert.deepEqual(server.value, { title: 'Other page', color: 'black' })
})

test('queued snapshots drop already-acknowledged field intents before rebasing a later edit', async (context) => {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const server = remote()
  const firstWrite = deferred<void>()
  let calls = 0
  let view: Config = server.value
  const sync = createPanelSync(view, {
    read: server.read,
    async write(value, revision) {
      if (++calls === 1) {
        await firstWrite.promise
        const result = await server.write(value, revision)
        server.replace({ ...server.value, title: 'Other page after first commit' })
        return result
      }
      return server.write(value, revision)
    },
    apply: value => { view = clone(value) },
  })
  await sync.load()
  sync.observe({ ...view, title: 'First local title' })
  const saved = sync.flush()
  await setImmediate()
  sync.observe({ title: 'First local title', color: 'green' })
  firstWrite.resolve()
  assert.equal(await saved, true)
  assert.deepEqual(server.value, { title: 'Other page after first commit', color: 'green' })
})
