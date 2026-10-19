import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function getValueByName<T>(name: string, options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/system/moduleConfig/getByName',
    data: { name },
  })
}

export function save<T>(name: string, value: unknown, revision: number, options: RequestOptions = {}) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/system/moduleConfig/save',
    data: {
      name,
      value,
      revision,
    },
  })
}
