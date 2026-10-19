import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export interface TrashRef {
  kind: 'item' | 'group'
  id: number
  deletionId?: string
}

export interface TrashEntry extends TrashRef {
  title: string
  deletedAt: string
  groupId?: number
  groupTitle?: string
  itemCount: number
  requiresTargetGroup: boolean
}

export interface TrashList {
  list: TrashEntry[]
  count: number
}

export function listTrash<T = TrashList>(options: RequestOptions = {}) {
  return post<T>({ ...options, operation: 'read', url: '/panel/navigation/trash/list', data: {} })
}

export function restoreTrash<T = unknown>(data: TrashRef & { targetGroupId?: number; targetIndex?: number; revision?: number }, options: RequestOptions = {}) {
  return post<T>({ ...options, operation: 'write', url: '/panel/navigation/trash/restore', data })
}

export function purgeTrash<T = unknown>(data: TrashRef & { revision?: number }, options: RequestOptions = {}) {
  return post<T>({ ...options, operation: 'write', url: '/panel/navigation/trash/purge', data })
}
