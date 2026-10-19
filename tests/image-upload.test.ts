import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'
import { isRequestCancelled, isUncertainWrite, RequestError } from '../src/utils/requestLifetime.ts'

interface UploadResponse { code: number; data: { imageUrl: string } }
interface UploadRequest {
  url: string
  data: FormData
  operation: string
  silent: boolean
  signal: AbortSignal
  onUploadProgress: (event: { progress?: number; total?: number; loaded: number }) => void
}
interface Hook {
  active: { value: number }
  error: { value: string }
  cancelled: { value: boolean }
  progress: { value: number | undefined }
  upload: (file: File) => void
  cancel: () => void
}

const source = readFileSync(new URL('../src/hooks/useImageUpload.ts', import.meta.url), 'utf8')
const javascript = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText

function harness() {
  const cleanup: Array<() => void> = []
  const requests: Array<{ options: UploadRequest; resolve: (response: UploadResponse) => void; reject: (error: unknown) => void }> = []
  const applied: string[] = []
  const module = { exports: {} as { useImageUpload: (uploaded: (data: { imageUrl: string }) => void) => Hook } }
  const require = (name: string) => {
    if (name === 'vue') return { ref: <T>(value: T) => ({ value }), onBeforeUnmount: (callback: () => void) => cleanup.push(callback) }
    if (name === '@/utils/requestLifetime') return { isRequestCancelled, isUncertainWrite }
    if (name === '@/utils/request') {
      return { post: (options: UploadRequest) => new Promise<UploadResponse>((resolve, reject) => { requests.push({ options, resolve, reject }) }) }
    }
    throw new Error(`Unexpected image-upload dependency: ${name}`)
  }
  // Execute the real hook, controlling only the network response and Vue owner.
  // The fake transport can deliberately return late after abort to test guards.
  runInNewContext(javascript, { require, module, exports: module.exports, AbortController, FormData, Error })
  const hook = module.exports.useImageUpload(data => applied.push(data.imageUrl))
  function begin(id: string) {
    const file = new File(['image bytes'], `${id}.png`, { type: 'image/png' })
    hook.upload(file)
    return { ...requests.at(-1)!, file }
  }
  return { hook, requests, applied, begin, unmount: () => cleanup.forEach(callback => callback()) }
}

async function settle() {
  for (let index = 0; index < 8; index++) await Promise.resolve()
}
const success = (imageUrl: string): UploadResponse => ({ code: 0, data: { imageUrl } })

test('image uploads send FormData with the transfer deadline and apply a successful response once', async () => {
  const { hook, begin, applied } = harness()
  const upload = begin('selected')
  assert.equal(upload.options.url, '/file/uploadImg')
  assert.equal(upload.options.operation, 'upload')
  assert.equal(upload.options.silent, true)
  const sent = upload.options.data.get('imgfile')
  assert.ok(sent instanceof File)
  assert.equal(sent.name, upload.file.name)
  assert.equal(sent.type, upload.file.type)
  assert.equal(await sent.text(), await upload.file.text())
  assert.equal(hook.active.value, 1)
  upload.options.onUploadProgress({ loaded: 3, total: 10 })
  assert.equal(hook.progress.value, 30)
  upload.resolve(success('/uploads/selected.png'))
  await settle()
  assert.deepEqual(applied, ['/uploads/selected.png'])
  assert.equal(hook.progress.value, 100)
  assert.equal(hook.active.value, 0)
  assert.equal(hook.error.value, '')
})

test('manual cancellation aborts the upload, suppresses progress and presents a quiet cancelled state', async () => {
  const { hook, begin, applied } = harness()
  const upload = begin('cancelled')
  hook.cancel()
  assert.equal(upload.options.signal.aborted, true)
  assert.equal(hook.active.value, 0, 'cancellation should finish the visible job even before the adapter settles')
  assert.equal(hook.cancelled.value, true)
  upload.options.onUploadProgress({ loaded: 10, total: 10 })
  upload.reject(new RequestError('cancelled', 'upload'))
  await settle()
  assert.deepEqual(applied, [])
  assert.equal(hook.progress.value, undefined)
  assert.equal(hook.active.value, 0)
  assert.equal(hook.cancelled.value, true)
  assert.equal(hook.error.value, '')
})

test('a newer selection cancels the previous image and an older late success cannot overwrite it', async () => {
  const { hook, begin, applied } = harness()
  const previous = begin('previous')
  const latest = begin('latest')
  assert.equal(previous.options.signal.aborted, true)
  assert.equal(latest.options.signal.aborted, false)
  previous.options.onUploadProgress({ loaded: 100, total: 100 })
  previous.resolve(success('/uploads/previous.png'))
  await settle()
  assert.deepEqual(applied, [])
  assert.equal(hook.progress.value, 0, 'late old progress must not affect the newer upload')
  assert.equal(hook.active.value, 1, 'retiring an old job must not clear the latest active job')
  assert.equal(hook.cancelled.value, false)
  latest.resolve(success('/uploads/latest.png'))
  await settle()
  assert.deepEqual(applied, ['/uploads/latest.png'])
  assert.equal(hook.progress.value, 100)
  assert.equal(hook.active.value, 0)
})

test('an old cancellation response does not overwrite the newer upload state', async () => {
  const { hook, begin, applied } = harness()
  const previous = begin('previous')
  const latest = begin('latest')
  previous.reject(new RequestError('cancelled', 'upload'))
  await settle()
  assert.equal(hook.cancelled.value, false)
  assert.equal(hook.error.value, '')
  assert.equal(hook.active.value, 1)
  assert.equal(hook.progress.value, 0)
  latest.resolve(success('/uploads/latest.png'))
  await settle()
  assert.deepEqual(applied, ['/uploads/latest.png'])
})

test('unmount aborts the owner upload and ignores late progress and successful results', async () => {
  const { hook, begin, applied, unmount } = harness()
  const upload = begin('closed')
  unmount()
  assert.equal(upload.options.signal.aborted, true)
  upload.options.onUploadProgress({ loaded: 10, total: 10 })
  upload.resolve(success('/uploads/closed.png'))
  await settle()
  assert.deepEqual(applied, [])
  assert.equal(hook.progress.value, undefined)
  assert.equal(hook.active.value, 0)
})

test('an unmounted owner does not emit a late cancellation or network error', async () => {
  const { hook, begin, applied, unmount } = harness()
  const upload = begin('closed')
  unmount()
  upload.reject(new RequestError('network', 'upload'))
  await settle()
  assert.deepEqual(applied, [])
  assert.equal(hook.progress.value, undefined)
  assert.equal(hook.cancelled.value, false)
  assert.equal(hook.error.value, '')
})

test('upload timeouts retain an uncertain outcome without automatic resubmission', async () => {
  const { hook, requests, begin, applied } = harness()
  const upload = begin('timeout')
  upload.reject(new RequestError('timeout', 'upload'))
  await settle()
  assert.equal(requests.length, 1)
  assert.deepEqual(applied, [])
  assert.equal(hook.error.value, '操作结果尚未确认，请先刷新检查，避免重复提交。')
  assert.equal(hook.cancelled.value, false)
  assert.equal(hook.active.value, 0)
  assert.equal(hook.progress.value, undefined)
})


test('upload progress remains indeterminate without a total and clamps transport ratios', async () => {
  const { hook, begin } = harness()
  const upload = begin('progress')
  assert.equal(hook.progress.value, 0)
  upload.options.onUploadProgress({ loaded: 20 })
  assert.equal(hook.progress.value, undefined)
  upload.options.onUploadProgress({ loaded: 20, progress: 1.2 })
  assert.equal(hook.progress.value, 100)
  upload.options.onUploadProgress({ loaded: 20, progress: -1 })
  assert.equal(hook.progress.value, 0)
  upload.options.onUploadProgress({ loaded: 20, progress: Number.NaN })
  assert.equal(hook.progress.value, undefined)
  upload.resolve(success('/uploads/progress.png'))
  await settle()
  assert.equal(hook.progress.value, 100)
})

test('a cancelled upload cannot apply a late successful response or interfere with a new selection', async () => {
  const { hook, begin, applied } = harness()
  const cancelled = begin('cancelled')
  hook.cancel()
  const latest = begin('latest')
  cancelled.resolve(success('/uploads/cancelled.png'))
  await settle()
  assert.deepEqual(applied, [])
  assert.equal(hook.active.value, 1)
  assert.equal(hook.cancelled.value, false)
  latest.resolve(success('/uploads/latest.png'))
  await settle()
  assert.deepEqual(applied, ['/uploads/latest.png'])
})
