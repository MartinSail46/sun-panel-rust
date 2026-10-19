import { formatDate } from '@/utils/date'

export function timeFormat(timeString?: string) {
  return formatDate(timeString)
}

export function setTitle(title: string) {
  document.title = title
}

// 复制文字到剪切板
export async function copyToClipboard(text: string): Promise<boolean> {
  if (navigator.clipboard) {
    // 使用 Clipboard API
    try {
      await navigator.clipboard.writeText(text)
      return true
    }
    catch (err) {
      console.error('copy fail', err)
      return false
    }
  }
  else {
    // 兼容旧版浏览器
    const textArea = document.createElement('textarea')
    textArea.value = text
    document.body.appendChild(textArea)
    textArea.select()

    try {
      return document.execCommand('copy')
    }
    catch (err) {
      console.error('copy fail', err)
      return false
    }
    finally {
      document.body.removeChild(textArea)
    }
  }
}

export function bytesToSize(bytes: number) {
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB']
  if (bytes === 0)
    return '0B'
  const i = parseInt(String(Math.floor(Math.log(bytes) / Math.log(1024))))
  return `${(bytes / 1024 ** i).toFixed(1)} ${sizes[i]}`
}
