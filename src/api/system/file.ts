import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function getList<T>(options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/file/getList',
  })
}

export function deletes<T>(ids: number[], options: RequestOptions = {}) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/file/deletes',
    data: { ids },
  })
}
