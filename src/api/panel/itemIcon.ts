import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function edit<T>(req: Panel.ItemInfo, options: RequestOptions = {}) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/itemIcon/edit',
    data: req,
  })
}

export function deletes<T>(ids: number[], options: RequestOptions = {}, revision?: number) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/itemIcon/deletes',
    data: { ids, revision },
  })
}

export function getSiteFavicon<T>(url: string, options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/panel/itemIcon/getSiteFavicon',
    data: { url },
  })
}
