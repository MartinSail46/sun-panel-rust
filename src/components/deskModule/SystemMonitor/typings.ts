export enum MonitorType {
  'cpu' = 'cpu', // 图标风格
  'memory' = 'memory', // 详情风格
  'disk' = 'disk',
}

export interface MonitorData {
  monitorType: MonitorType
  extendParam?: { [key: string]: [value:any] } | any
  description?: string
}

export interface GenericProgressStyleExtendParam {
  style?: 0 | 1
  progressColor: string
  progressRailColor: string
  color: string
  backgroundColor: string
}

export interface DiskExtendParam extends GenericProgressStyleExtendParam {
  path: string
}
