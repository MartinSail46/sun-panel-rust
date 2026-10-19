import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function edit<T>(req: Panel.ItemIconGroup, options: RequestOptions = {}) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/itemIconGroup/edit',
    data: req,
  })
}

export function getList<T>(options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/panel/itemIconGroup/getList',
  })
}

export function deletes<T>(ids: number[], options: RequestOptions = {}, revision?: number) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/itemIconGroup/deletes',
    data: { ids, revision },
  })
}

export function saveSort<T>(sortItems: Common.SortItemRequest[], options: RequestOptions = {}, revision?: number) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/itemIconGroup/saveSort',
    data: { sortItems, revision },
  })
}
