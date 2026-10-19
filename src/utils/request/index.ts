import type { AxiosProgressEvent, AxiosResponse } from 'axios'
import request from './axios'
import { apiRespErrMsg } from './apiMessage'
import { toast } from '@/lib/feedback'
import { RequestError, requestFailureMessage, runRequestLifecycle } from '@/utils/requestLifetime'
import type { RequestOptions } from '@/utils/requestLifetime'

export type { RequestOptions } from '@/utils/requestLifetime'

interface HttpOption extends RequestOptions {
  url: string
  data?: any
  headers?: any
  onUploadProgress?: (progressEvent: AxiosProgressEvent) => void
}

export interface Response<T = any> {
  revision?: number
  currentRevision?: number
  data: T
  msg: string
  code: number
}

export function post<T = any>(
  { url, data, headers, onUploadProgress, signal, timeoutMs, operation = 'write', silent }: HttpOption,
) {
  const successHandler = (res: AxiosResponse<Response<T>>) => {
    if (res.data.code === 0)
      return res.data
    if (silent)
      return res.data
    // Version conflicts must reach the central save coordinator; a global toast
    // cannot resolve them and must not convert a failed save to success.
    if (res.data.code === 1409)
      return Promise.reject(res.data)

    if (!apiRespErrMsg(res.data))
      return Promise.reject(res.data)
    else
      return res.data
  }

  const failHandler = (error: unknown) => {
    const message = requestFailureMessage(error, silent)
    if (message && error instanceof RequestError) {
      const text = message + (error.outcomeUnknown ? ' 操作结果尚未确认，请先刷新检查，避免重复提交。' : '')
      toast.error(text, {
        duration: 5000,
      })
    }
    throw error
  }

  const params = Object.assign(typeof data === 'function' ? data() : data ?? {}, {})
  if (!headers)
    headers = {}

  return runRequestLifecycle(
    requestSignal => request.post(url, params, { headers, signal: requestSignal, onUploadProgress }),
    { signal, timeoutMs, operation },
    successHandler,
  ).catch(failHandler)
}
