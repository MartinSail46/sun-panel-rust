import { post } from '@/utils/request'
import type { RequestOptions } from '@/utils/request'
import type { NavigationMoveRequest } from '@/utils/navigationMove'

export function getNavigation<T>(options: RequestOptions = {}) {
  return post<T>({ operation: 'read', ...options, url: '/panel/navigation' })
}

export function importNavigation<T>(data: unknown, options: RequestOptions = {}) {
  return post<T>({ operation: 'import', ...options, url: '/panel/navigation/import', data })
}

export function previewNavigation<T>(data: unknown, options: RequestOptions = {}) {
  return post<T>({ operation: 'read', timeoutMs: 35_000, ...options, url: '/panel/navigation/import/preview', data })
}

export function exportNavigation<T>(options: RequestOptions = {}) {
  return post<T>({ operation: 'read', ...options, url: '/panel/navigation/export' })
}

export function moveNavigation<T>(data: NavigationMoveRequest, options: RequestOptions = {}) {
  return post<T>({ ...options, url: '/panel/navigation/move', data })
}
