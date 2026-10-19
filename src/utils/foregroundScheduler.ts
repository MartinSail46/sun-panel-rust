export interface SchedulerTimer {
  now: () => number
  setTimeout: (callback: () => void, delay: number) => unknown
  clearTimeout: (handle: unknown) => void
}

interface ForegroundSchedulerOptions {
  run: (signal: AbortSignal) => void | Promise<void>
  interval: () => number
  isVisible: () => boolean
  timer?: SchedulerTimer
  successDelay?: (now: number) => number
  maxBackoff?: number
  onError?: (error: unknown) => void
}

// One timer and one in-flight task. Hidden pages do no new work, and a visible
// page immediately catches up without overlapping a request already in flight.
export function createForegroundScheduler(options: ForegroundSchedulerOptions) {
  const timer: SchedulerTimer = options.timer ?? {
    now: Date.now,
    setTimeout: (callback, delay) => globalThis.setTimeout(callback, delay),
    clearTimeout: handle => globalThis.clearTimeout(handle as ReturnType<typeof setTimeout>),
  }
  let active = false
  let inFlight = false
  let refreshRequested = false
  let failures = 0
  let timeout: unknown
  let controller: AbortController | undefined

  function clearTimer() {
    if (timeout !== undefined) {
      timer.clearTimeout(timeout)
      timeout = undefined
    }
  }

  function schedule(delay: number) {
    if (active && options.isVisible() && !inFlight && timeout === undefined)
      timeout = timer.setTimeout(() => { void tick() }, Math.max(0, delay))
  }

  async function tick() {
    timeout = undefined
    if (!active || !options.isVisible() || inFlight)
      return

    inFlight = true
    const current = new AbortController()
    controller = current
    try {
      await options.run(current.signal)
      failures = 0
    }
    catch (error) {
      if (!current.signal.aborted) {
        failures++
        options.onError?.(error)
      }
    }
    finally {
      inFlight = false
      if (controller === current)
        controller = undefined
      const interval = Math.max(1, options.interval())
      const delay = refreshRequested
        ? 0
        : failures
          ? Math.min(interval * 2 ** Math.min(failures, 10), options.maxBackoff ?? 60000)
          : options.successDelay?.(timer.now()) ?? interval
      refreshRequested = false
      schedule(delay)
    }
  }

  function refresh() {
    clearTimer()
    if (!active || !options.isVisible())
      return
    failures = 0
    if (inFlight)
      refreshRequested = true
    else
      schedule(0)
  }

  return {
    start() {
      if (!active) {
        active = true
        refresh()
      }
    },
    stop() {
      active = false
      refreshRequested = false
      clearTimer()
      controller?.abort()
    },
    refresh,
    visibilityChanged() {
      if (options.isVisible())
        refresh()
      else {
        refreshRequested = false
        clearTimer()
        controller?.abort()
      }
    },
  }
}

export function nextClockBoundary(now: number, hideSecond: boolean) {
  const interval = hideSecond ? 60000 : 1000
  return interval - now % interval
}
