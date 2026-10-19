import type { MonitorData } from './typings'
import { equalJson } from '../../../utils/confirmedWrite.ts'

interface OrderStorage {
  save: (value: MonitorData[]) => Promise<{ code: number; msg?: string }>
  read: () => Promise<MonitorData[]>
}

export interface OrderResult {
  value: MonitorData[]
  confirmed: boolean
  needsReload: boolean
  error?: unknown
}

const copy = (value: MonitorData[]) => JSON.parse(JSON.stringify(value)) as MonitorData[]

// A failed/uncertain write is never replayed. Read once to show the actual
// server order; if that read fails, retain the last confirmed order and lock
// edits until a later explicit reload succeeds.
export async function persistMonitorOrder(before: MonitorData[], ordered: MonitorData[], storage: OrderStorage): Promise<OrderResult> {
  const snapshot = copy(ordered)
  if (equalJson(before, snapshot))
    return { value: snapshot, confirmed: true, needsReload: false }
  try {
    const response = await storage.save(snapshot)
    if (response.code !== 0)
      throw new Error(response.msg || '监控排序保存失败')
    return { value: snapshot, confirmed: true, needsReload: false }
  }
  catch (error) {
    try {
      const actual = copy(await storage.read())
      return { value: actual, confirmed: equalJson(actual, snapshot), needsReload: false, error }
    }
    catch {
      return { value: copy(before), confirmed: false, needsReload: true, error }
    }
  }
}
