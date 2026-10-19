export type RequestOperation = 'read' | 'write' | 'import' | 'upload'
export type RequestFailure = 'timeout' | 'cancelled' | 'network'

export const REQUEST_TIMEOUTS: Record<RequestOperation, number> = {
  read: 10_000,
  write: 15_000,
  import: 35_000,
  upload: 35_000,
}

export class RequestError extends Error {
  readonly name = 'RequestError'
  readonly kind: RequestFailure
  readonly outcomeUnknown: boolean

  constructor(kind: RequestFailure, operation: RequestOperation, cause?: unknown) {
    super(kind === 'timeout' ? '请求超时，请稍后重试' : kind === 'cancelled' ? '加载已取消' : '网络错误，请稍后重试', { cause })
    this.kind = kind
    // A disconnected client cannot tell whether the server has committed a write.
    this.outcomeUnknown = operation !== 'read'
  }
}

export function isRequestCancelled(error: unknown): error is RequestError {
  return error instanceof RequestError && error.kind === 'cancelled'
}

export function isUncertainWrite(error: unknown): error is RequestError {
  return error instanceof RequestError && error.outcomeUnknown
}

export function requestFailureMessage(error: unknown, silent = false) {
  if (silent || !(error instanceof RequestError) || error.kind === 'cancelled')
    return undefined
  return error.message
}

export interface RequestOptions {
  signal?: AbortSignal
  timeoutMs?: number
  operation?: RequestOperation
  silent?: boolean
}

interface LifecycleOptions extends RequestOptions {
  beforeRequest?: () => void
  afterRequest?: () => void
}

/** A wall-clock deadline, including connection stalls and adapters ignoring abort. */
export async function runRequestLifecycle<T, R = T>(
  request: (signal: AbortSignal) => Promise<T>,
  options: LifecycleOptions,
  handleResponse: (response: T) => R | Promise<R> = response => response as unknown as R,
): Promise<R> {
  const operation = options.operation ?? 'read'
  const timeoutMs = options.timeoutMs ?? REQUEST_TIMEOUTS[operation]
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0)
    throw new RangeError('A positive, finite request deadline is required')

  const controller = new AbortController()
  let deadline: ReturnType<typeof setTimeout> | undefined
  let failure: RequestError | undefined
  let rejectAbort!: (error: RequestError) => void
  const interrupted = new Promise<never>((_, reject) => { rejectAbort = reject })
  const interrupt = (kind: 'cancelled' | 'timeout') => {
    if (failure)
      return
    failure = new RequestError(kind, operation, options.signal?.reason)
    // Settle the deadline before calling abort: an adapter may reject synchronously.
    rejectAbort(failure)
    controller.abort(failure)
  }
  const onExternalAbort = () => interrupt('cancelled')

  try {
    options.beforeRequest?.()
    if (options.signal?.aborted) {
      // Do not start either the adapter or an unhandled rejected race promise.
      throw new RequestError('cancelled', operation, options.signal.reason)
    }
    options.signal?.addEventListener('abort', onExternalAbort, { once: true })
    deadline = setTimeout(interrupt, timeoutMs, 'timeout')
    let response: T
    try {
      response = await Promise.race([Promise.resolve().then(() => {
        if (controller.signal.aborted)
          throw failure
        return request(controller.signal)
      }), interrupted])
    }
    catch (error) {
      if (failure)
        throw failure
      if (error instanceof RequestError)
        throw error
      const code = error && typeof error === 'object' && 'code' in error ? error.code : undefined
      throw new RequestError(code === 'ERR_CANCELED' ? 'cancelled' : code === 'ECONNABORTED' || code === 'ETIMEDOUT' ? 'timeout' : 'network', operation, error)
    }
    // Business failures (including revision conflicts) retain their original object.
    return await handleResponse(response)
  }
  finally {
    if (deadline !== undefined)
      clearTimeout(deadline)
    options.signal?.removeEventListener('abort', onExternalAbort)
    options.afterRequest?.()
  }
}
