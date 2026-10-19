import { isUncertainWrite } from './requestLifetime.ts'
import type { RequestError } from './requestLifetime.ts'

interface Snapshot<T> { value: T; revision: number }
interface Options<T> {
  read: () => Promise<Snapshot<T>>
  write: (value: T, revision: number) => Promise<number>
}

export function equalJson(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (!a || !b || typeof a !== 'object' || typeof b !== 'object') return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const left = Object.keys(a)
  const right = Object.keys(b)
  return left.length === right.length && left.every(key => Object.hasOwn(b, key) && equalJson((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]))
}

/** Never replay a disconnected write. Read back first, including before a later explicit retry. */
export function createConfirmedWriter<T>(options: Options<T>) {
  let uncertain: RequestError | undefined
  return async (value: T, revision: number): Promise<number> => {
    if (uncertain) {
      let remote: Snapshot<T>
      try { remote = await options.read() }
      catch { throw uncertain }
      if (equalJson(remote.value, value)) {
        uncertain = undefined
        return remote.revision
      }
      if (remote.revision !== revision) {
        const msg = '配置已更新，请重新载入后再保存。'
        throw Object.assign(new Error(msg), { code: 1409, msg, currentRevision: remote.revision })
      }
      // A later user-requested write may proceed using the revision just read.
      // Compare-and-swap still prevents an older server request from overwriting it.
      uncertain = undefined
    }
    try { return await options.write(value, revision) }
    catch (error) {
      if (!isUncertainWrite(error)) throw error
      uncertain = error
      try {
        const remote = await options.read()
        if (equalJson(remote.value, value)) {
          uncertain = undefined
          return remote.revision
        }
      }
      catch { /* The original uncertain outcome is more useful than a failed verification read. */ }
      throw error
    }
  }
}
