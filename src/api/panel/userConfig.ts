import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function set<T>(req: { panel: Panel.panelConfig; revision: number }, options: RequestOptions = {}) {
  return post<T>({
    operation: 'write',
    ...options,
    url: '/panel/userConfig/set',
    data: req,
  })
}

export function get<T>(options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/panel/userConfig/get',
  })
}
