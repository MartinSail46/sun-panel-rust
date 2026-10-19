import type { Response } from './index'
import { toast } from '@/lib/feedback'

const apiErrorMessages: Record<number, string> = {
  1005: '此操作不被允许',
  1200: '数据库出错',
  1201: '请至少保留一个',
  1300: '上传失败',
  1301: '上传失败，不被支持的文件格式',
  1310: '图片正在使用，请先替换引用后再删除。',
  1311: '图片已被删除，请选择其他图片。',
  1400: '参数格式错误',
  1409: '配置已被其他页面更新，请刷新后重试。',
}

export function apiRespErrMsg(res: Response): boolean {
  const text = apiErrorMessages[res.code]
  if (!text) return false
  toast.error(text)
  return true
}
