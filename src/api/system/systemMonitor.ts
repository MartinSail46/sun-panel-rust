import { post } from '@/utils/request'
import type { RequestOptions} from '@/utils/request'

export interface MonitorSnapshot {
  sampledAt?: string
  scope?: { source: string, cpu: string, memory: string, disk: string, containerQuotaApplied: boolean }
  cpuInfo: SystemMonitor.CPUInfo
  memoryInfo: SystemMonitor.MemoryInfo
  disks: Record<string, { data?: SystemMonitor.DiskInfo, error?: string }>
}

export function getMonitorSnapshot(diskPaths: string[], options: RequestOptions = {}) {
  return post<MonitorSnapshot>({
    operation: 'read',
    ...options,
    url: '/system/monitor/snapshot',
    silent: true,
    data: { diskPaths },
  })
}

export function getDiskMountpoints<T>(options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/system/monitor/getDiskMountpoints',
  })
}
