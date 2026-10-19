/** Format local time using the numeric tokens used by the panel and its backups. */
export function formatDate(value: Date | string | number = new Date(), pattern = 'YYYY-MM-DD HH:mm:ss'): string {
  let normalizedValue = value
  if (typeof value === 'string') {
    // SQLite timestamps have a space separator; date-only values represent local midnight.
    normalizedValue = value.replace(/^(\d{4}-\d{2}-\d{2}) /, '$1T')
    if (/^\d{4}-\d{2}-\d{2}$/.test(normalizedValue))
      normalizedValue += 'T00:00:00'
  }

  const date = normalizedValue instanceof Date ? normalizedValue : new Date(normalizedValue)
  if (Number.isNaN(date.getTime()))
    return '日期无效'

  const tokens: Record<string, string> = {
    YYYY: String(date.getFullYear()).padStart(4, '0'),
    MM: String(date.getMonth() + 1).padStart(2, '0'),
    DD: String(date.getDate()).padStart(2, '0'),
    HH: String(date.getHours()).padStart(2, '0'),
    mm: String(date.getMinutes()).padStart(2, '0'),
    ss: String(date.getSeconds()).padStart(2, '0'),
  }

  return pattern.replace(/YYYY|MM|DD|HH|mm|ss/g, token => tokens[token])
}
