export interface SearchableItem {
  title: string
  url: string
  description?: string
}

export function cloneDraft<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

export function filterNavigation<T extends { items: SearchableItem[] }>(groups: T[], term: string): T[] {
  const keyword = term.trim().toLocaleLowerCase()
  if (!keyword)
    return groups
  return groups.map(group => ({
    ...group,
    items: group.items.filter(item => [item.title, item.url, item.description || ''].some(value => value.toLocaleLowerCase().includes(keyword))),
  })).filter(group => group.items.length)
}

export function moved<T>(items: T[], from: number, delta: number): T[] {
  const to = from + delta
  if (from < 0 || from >= items.length || to < 0 || to >= items.length)
    return items
  const result = [...items]
  const [item] = result.splice(from, 1)
  result.splice(to, 0, item)
  return result
}

export function selectionIndex(current: number, delta: number, count: number): number {
  if (!count)
    return -1
  return ((current < 0 ? (delta > 0 ? -1 : 0) : current) + delta + count) % count
}

export function searchUrl(url: string, term: string): string {
  const keyword = encodeURIComponent(term.trim())
  return url.includes('%s') ? url.replace('%s', keyword) : url + keyword
}
