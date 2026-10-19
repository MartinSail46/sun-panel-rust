export type SaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error'

export interface SaveState {
  status: SaveStatus
  error?: string
}

export interface SaveQueue<T> {
  enqueue: (value: T) => void
  flush: () => Promise<boolean>
  subscribe: (listener: (state: SaveState) => void) => () => void
  discardPending: () => boolean
}

interface Snapshot<T> {
  key: string
  value: T
}

/** Debounce JSON settings, serialize writes and retain the latest failed snapshot for retry. */
export function createSaveQueue<T>(save: (value: T) => Promise<void>, debounceMs = 700): SaveQueue<T> {
  let pending: Snapshot<T> | undefined
  let active: Snapshot<T> | undefined
  let savedKey: string | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  let ready = false
  let flushing = false
  let state: SaveState = { status: 'idle' }
  const listeners = new Set<(state: SaveState) => void>()
  const waiters: Array<(success: boolean) => void> = []

  function publish(next: SaveState) {
    state = next
    for (const listener of listeners)
      listener(state)
  }

  function clearTimer() {
    clearTimeout(timer)
    timer = undefined
  }

  function settle(success: boolean) {
    flushing = false
    for (const resolve of waiters.splice(0))
      resolve(success)
  }

  async function drain() {
    if (active || !pending || !ready)
      return

    active = pending
    pending = undefined
    ready = false
    publish({ status: 'saving' })
    try {
      await save(active.value)
      savedKey = active.key
    }
    catch (error) {
      // A newer pending value supersedes the failed request; never retry stale settings.
      pending ??= active
      active = undefined
      ready = false
      clearTimer()
      const message = error instanceof Error
        ? error.message
        : typeof error === 'object' && error !== null && 'msg' in error
          ? String(error.msg)
          : String(error)
      publish({ status: 'error', error: message })
      settle(false)
      return
    }
    active = undefined

    if (pending) {
      publish({ status: 'pending' })
      if (ready)
        void drain()
    }
    else {
      publish({ status: 'saved' })
      settle(true)
    }
  }

  function enqueue(value: T) {
    // Clone here so later reactive mutations cannot alter an in-flight request body.
    const key = JSON.stringify(value)
    if (pending?.key === key)
      return

    clearTimer()
    if (active?.key === key || (!active && savedKey === key)) {
      pending = undefined
      ready = false
      publish({ status: active ? 'saving' : 'saved' })
      if (!active)
        settle(true)
      return
    }

    pending = { key, value: JSON.parse(key) as T }
    ready = flushing
    publish({ status: active ? 'saving' : 'pending' })
    if (flushing) {
      void drain()
    }
    else {
      timer = setTimeout(() => {
        timer = undefined
        ready = true
        void drain()
      }, debounceMs)
    }
  }

  function flush(): Promise<boolean> {
    clearTimer()
    if (!pending && !active)
      return Promise.resolve(true)

    flushing = true
    ready = true
    const completed = new Promise<boolean>(resolve => waiters.push(resolve))
    void drain()
    return completed
  }

  return {
    enqueue,
    flush,
    discardPending() {
      if (active)
        return false
      clearTimer()
      pending = undefined
      ready = false
      savedKey = undefined
      settle(false)
      publish({ status: 'idle' })
      return true
    },
    subscribe(listener) {
      listeners.add(listener)
      listener(state)
      return () => listeners.delete(listener)
    },
  }
}
