import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'

export function get<T>(options: RequestOptions = {}) {
  return post<T>({
    operation: 'read',
    ...options,
    url: '/about',
  })
}
