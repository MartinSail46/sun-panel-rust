import { createSaveQueue } from './settingsSave.ts'
import type { SaveState } from './settingsSave.ts'

type Document = Record<string, unknown>
interface Change { present: boolean; value?: unknown; sequence: number }
interface Remote<T> { value: T; revision: number }
interface Adapter<T> {
  read: () => Promise<Remote<T>>
  write: (value: T, revision: number) => Promise<number>
  apply: (value: T) => void
  published?: (revision: number) => void
  conflictMessage?: () => string
}
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))
const equal = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)
const present = (value: object, key: string) => Object.hasOwn(value, key)

export function createPanelSync<T extends object>(initial: T, adapter: Adapter<T>) {
  let observed = clone(initial)
  let server = clone(initial)
  let revision = 0
  let loaded = false
  let loading: Promise<void> | undefined
  let applying = false
  let sequence = 0
  let state: SaveState = { status: 'idle' }
  const pending = new Map<string, Change>()

  function merge(base: T, changes: Record<string, Change>): T {
    const result = clone(base) as Document
    for (const [key, change] of Object.entries(changes)) {
      if (change.present)
        result[key] = change.value
      else
        delete result[key]
    }
    return result as T
  }
  function apply() {
    const value = merge(server, Object.fromEntries(pending))
    applying = true
    try {
      observed = clone(value)
      adapter.apply(value)
    }
    finally { applying = false }
  }
  async function load() {
    if (loaded)
      return
    if (loading)
      return loading
    loading = adapter.read().then((remote) => {
      server = clone(remote.value)
      revision = remote.revision
      loaded = true
      apply()
    }).finally(() => { loading = undefined })
    return loading
  }
  const queue = createSaveQueue<Record<string, Change>>(async (changes) => {
    await load()
    // A queued snapshot may still contain fields acknowledged by the previous
    // request, or superseded while the initial read was in flight.
    changes = Object.fromEntries(Object.entries(changes).filter(([key, change]) => pending.get(key)?.sequence === change.sequence))
    if (!Object.keys(changes).length)
      return
    let next = merge(server, changes)
    let nextRevision: number
    try { nextRevision = await adapter.write(next, revision) }
    catch (error) {
      if (!error || typeof error !== 'object' || !('code' in error) || error.code !== 1409)
        throw error
      const remote = await adapter.read()
      const conflicts = Object.entries(changes).filter(([key, change]) => {
        const changedElsewhere = !equal((server as Document)[key], (remote.value as Document)[key])
          || present(server, key) !== present(remote.value, key)
        const alreadyDesired = equal(change.value, (remote.value as Document)[key])
          && change.present === present(remote.value, key)
        return changedElsewhere && !alreadyDesired
      })
      if (conflicts.length)
        throw new Error(adapter.conflictMessage?.() || '其他页面修改了同一设置，请载入服务器配置后重新编辑。')
      // Rebase only locally edited fields; unrelated remote edits are preserved.
      server = clone(remote.value)
      revision = remote.revision
      apply()
      next = merge(server, changes)
      nextRevision = await adapter.write(next, revision)
    }
    server = clone(next)
    revision = nextRevision
    for (const [key, change] of Object.entries(changes)) {
      if (pending.get(key)?.sequence === change.sequence)
        pending.delete(key)
    }
    apply()
    adapter.published?.(revision)
  })
  queue.subscribe(value => { state = value })

  return {
    load,
    observe(value: T) {
      const next = clone(value)
      if (applying) {
        observed = next
        return
      }
      for (const key of new Set([...Object.keys(observed), ...Object.keys(next)])) {
        if (!equal((observed as Document)[key], (next as Document)[key]) || present(observed, key) !== present(next, key))
          pending.set(key, { present: present(next, key), value: (next as Document)[key], sequence: ++sequence })
      }
      observed = next
      if (pending.size)
        queue.enqueue(Object.fromEntries(pending))
    },
    flush: queue.flush,
    subscribe: queue.subscribe,
    async reload() {
      if (state.status === 'saving')
        return false
      const generation = sequence
      const remote = await adapter.read()
      if (generation !== sequence || !queue.discardPending())
        return false
      pending.clear()
      server = clone(remote.value)
      revision = remote.revision
      loaded = true
      apply()
      return true
    },
    async refreshIfClean() {
      if (!pending.size && state.status !== 'saving')
        await this.reload()
    },
  }
}
