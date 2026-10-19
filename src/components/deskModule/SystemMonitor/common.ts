import type { MonitorData } from './typings'
import { MonitorType } from './typings'
import { useModuleConfig } from '@/store/modules'

const modelName = 'systemMonitor'
const moduleConfig = useModuleConfig()
let loaded: MonitorData[] | undefined
const copy = (value: MonitorData[]) => JSON.parse(JSON.stringify(value)) as MonitorData[]

export async function saveAll(value: MonitorData[]) {
  const snapshot = copy(value)
  const response = await moduleConfig.saveToCloud(modelName, { list: snapshot })
  if (response.code === 0)
    loaded = snapshot
  return response
}

export async function getAll(): Promise<MonitorData[]> {
  const response = await moduleConfig.getValueByNameFromCloud<{ list: MonitorData[] }>(modelName)
  if (response.code !== 0)
    throw new Error(response.msg || '无法加载监控配置')
  loaded = Array.isArray(response.data?.list) && response.data.list.length ? copy(response.data.list) : [{
    extendParam: { backgroundColor: '#2a2a2a6b', color: '#fff', progressColor: '#fff', progressRailColor: '#CFCFCFA8' },
    monitorType: MonitorType.cpu,
  }]
  // The default participates in the first explicit edit, without a write on
  // page load. Adding RAM to an empty workspace therefore keeps its CPU card.
  return copy(loaded)
}

async function editLoaded(change: (value: MonitorData[]) => void): Promise<boolean> {
  try {
    // Preserve the revision that produced the visible list. Re-fetching here
    // would attach an old array index to a newer list and bypass conflict checks.
    if (!loaded)
      await getAll()
    const snapshot = copy(loaded || [])
    change(snapshot)
    return (await saveAll(snapshot)).code === 0
  }
  catch {
    return false
  }
}

export function add(value: MonitorData): Promise<boolean> {
  return editLoaded(list => { list.push(value) })
}

export function saveByIndex(index: number | undefined, value: MonitorData): Promise<boolean> {
  return editLoaded((list) => {
    const target = index ?? 0
    if (target < 0 || target > list.length)
      throw new Error('该监控卡片已不存在')
    list[target] = value
  })
}

export function deleteByIndex(index: number): Promise<boolean> {
  return editLoaded((list) => {
    if (!list[index])
      throw new Error('该监控卡片已不存在')
    list.splice(index, 1)
  })
}
