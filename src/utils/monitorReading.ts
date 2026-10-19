export interface MonitorReading<T> {
  value: T | null
  loading: boolean
  error: string | null
  updatedAt: number | null
}

export function createMonitorReading<T>(): MonitorReading<T> {
  return { value: null, loading: true, error: null, updatedAt: null }
}

export function acceptMonitorReading<T>(reading: MonitorReading<T>, value: T, sampledAt?: string, now = Date.now()) {
  reading.value = value
  reading.loading = false
  reading.error = null
  const parsed = sampledAt ? Date.parse(sampledAt) : Number.NaN
  reading.updatedAt = Number.isFinite(parsed) ? parsed : now
}

export function rejectMonitorReading<T>(reading: MonitorReading<T>, error: unknown) {
  reading.loading = false
  reading.error = error instanceof Error ? error.message : String(error || '系统监控暂不可用')
  // Keep both the last value and its timestamp: an unsuccessful request is not
  // a new measurement and must never turn an unknown metric into a zero.
}

export function monitorReadingIsStale<T>(reading: MonitorReading<T>, interval: number, now = Date.now()) {
  return !!reading.error || (reading.updatedAt !== null && now - reading.updatedAt > Math.max(15000, interval * 2))
}

export function formatMonitorCapacity(reading: { used: number, total: number } | null, formatBytes: (value: number) => string) {
  return reading ? `${formatBytes(reading.used)} / ${formatBytes(reading.total)}` : '—'
}
