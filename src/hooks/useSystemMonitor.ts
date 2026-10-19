import { computed, onMounted, onUnmounted, ref, shallowReactive, watch } from 'vue'
import { getMonitorSnapshot } from '@/api/system/systemMonitor'
import { createForegroundScheduler } from '@/utils/foregroundScheduler'
import { acceptMonitorReading, createMonitorReading, monitorReadingIsStale, rejectMonitorReading } from '@/utils/monitorReading'
import type { MonitorReading } from '@/utils/monitorReading'
import { isRequestCancelled } from '@/utils/requestLifetime'

type MonitorRequest = { type: 'cpu' | 'memory' } | { type: 'disk', path: string }
type MonitorValue = SystemMonitor.CPUInfo | SystemMonitor.MemoryInfo | SystemMonitor.DiskInfo
interface Subscriber {
  request: () => MonitorRequest
  interval: () => number
  start: () => void
  update: (value: MonitorValue, sampledAt?: string) => void
  fail: (error: unknown) => void
}

const subscribers = new Set<Subscriber>()

const scheduler = createForegroundScheduler({
  interval: () => Math.min(...Array.from(subscribers, subscriber => Math.max(2000, subscriber.interval() || 5000))),
  isVisible: () => document.visibilityState !== 'hidden',
  async run(signal) {
    const requested = Array.from(subscribers, subscriber => ({ subscriber, request: subscriber.request() }))
    for (const { subscriber } of requested)
      subscriber.start()
    let invalidPath = false
    const current = requested.filter(({ subscriber, request }) => {
      if (request.type === 'disk' && (!request.path || !request.path.startsWith('/'))) {
        subscriber.fail(new Error('请填写以 / 开头的磁盘绝对路径'))
        invalidPath = true
        return false
      }
      return true
    })
    if (!current.length)
      throw new Error('请填写以 / 开头的磁盘绝对路径')
    const paths = Array.from(new Set(current.flatMap(({ request }) => request.type === 'disk' ? [request.path] : [])))
    const batches = paths.length
      ? Array.from({ length: Math.ceil(paths.length / 16) }, (_, index) => paths.slice(index * 16, (index + 1) * 16))
      : [[]]
    const tasks = batches.map(async (diskPaths, index) => {
      const affected = current.filter(({ request }) => request.type === 'disk' ? diskPaths.includes(request.path) : index === 0)
      try {
        const { code, data, msg } = await getMonitorSnapshot(diskPaths, { signal })
        if (code !== 0)
          throw new Error(msg || '系统监控请求失败')
        let diskError: Error | undefined
        for (const { subscriber, request } of affected) {
          if (!subscribers.has(subscriber))
            continue
          const latest = subscriber.request()
          if (latest.type !== request.type || (latest.type === 'disk' && request.type === 'disk' && latest.path !== request.path))
            continue
          if (request.type !== 'disk') {
            subscriber.update(request.type === 'cpu' ? data.cpuInfo : data.memoryInfo, data.sampledAt)
          }
          else {
            const disk = data.disks[request.path]
            if (disk?.data)
              subscriber.update(disk.data, data.sampledAt)
            else {
              diskError = new Error(disk?.error || `磁盘不可用：${request.path}`)
              subscriber.fail(diskError)
            }
          }
        }
        // One failed disk backs off the batch while successful cards retain
        // their own healthy state and fresh measurement.
        if (diskError)
          return { error: diskError }
        return {}
      }
      catch (error) {
        if (isRequestCancelled(error))
          return {}
        for (const { subscriber, request } of affected) {
          const latest = subscriber.request()
          if (subscribers.has(subscriber) && latest.type === request.type && (latest.type !== 'disk' || request.type !== 'disk' || latest.path === request.path))
            subscriber.fail(error)
        }
        return { error }
      }
    })
    const results = await Promise.all(tasks)
    const failure = results.find(result => result.error)
    if (failure)
      throw failure.error
    if (invalidPath)
      throw new Error('请填写以 / 开头的磁盘绝对路径')
  },
})

function handleVisibilityChange() {
  scheduler.visibilityChanged()
}

export function useSystemMonitor<T extends MonitorValue>(request: () => MonitorRequest, interval: () => number) {
  const state = shallowReactive<MonitorReading<T>>(createMonitorReading<T>())
  const observedAt = ref(Date.now())
  const subscriber: Subscriber = {
    request,
    interval,
    start: () => {
      observedAt.value = Date.now()
      state.loading = true
    },
    update: (value, sampledAt) => { acceptMonitorReading(state, value as T, sampledAt) },
    fail: error => { rejectMonitorReading(state, error) },
  }
  onMounted(() => {
    subscribers.add(subscriber)
    if (subscribers.size === 1) {
      document.addEventListener('visibilitychange', handleVisibilityChange)
      scheduler.start()
    }
    else {
      scheduler.refresh()
    }
  })
  watch(request, () => {
    // A reused disk card must not show the previous path's reading.
    Object.assign(state, createMonitorReading<T>())
    if (subscribers.has(subscriber))
      scheduler.refresh()
  }, { deep: true })
  watch(interval, () => {
    if (subscribers.has(subscriber))
      scheduler.refresh()
  })
  onUnmounted(() => {
    subscribers.delete(subscriber)
    if (!subscribers.size) {
      scheduler.stop()
      document.removeEventListener('visibilitychange', handleVisibilityChange)
    }
  })
  return {
    state,
    stale: computed(() => monitorReadingIsStale(state, interval(), observedAt.value)),
    retry: () => scheduler.refresh(),
  }
}
