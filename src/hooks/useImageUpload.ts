import { onBeforeUnmount, ref } from 'vue'
import { post } from '@/utils/request'
import { isRequestCancelled, isUncertainWrite } from '@/utils/requestLifetime'

export function useImageUpload(onUploaded: (data: { imageUrl: string }) => void) {
  const active = ref(0)
  const error = ref('')
  const cancelled = ref(false)
  const progress = ref<number | undefined>(undefined)
  let current: AbortController | undefined
  let alive = true

  function upload(file: File) {
    if (!alive)
      return
    // Each caller edits one image. A newer selection supersedes the old transfer.
    current?.abort()
    const controller = new AbortController()
    current = controller
    active.value = 1
    progress.value = 0
    error.value = ''
    cancelled.value = false
    const data = new FormData()
    data.append('imgfile', file, file.name)
    void post<{ imageUrl: string }>({
      url: '/file/uploadImg', data, operation: 'upload', signal: controller.signal, silent: true,
      onUploadProgress: (event) => {
        if (!alive || current !== controller || controller.signal.aborted)
          return
        const ratio = event.progress ?? (event.total ? event.loaded / event.total : undefined)
        progress.value = ratio !== undefined && Number.isFinite(ratio)
          ? Math.min(100, Math.max(0, Math.round(ratio * 100)))
          : undefined
      },
    }).then((response) => {
      if (!alive || controller.signal.aborted || current !== controller)
        return
      if (response.code !== 0)
        throw response
      onUploaded(response.data)
      progress.value = 100
    }).catch((cause: unknown) => {
      if (!alive || current !== controller)
        return
      progress.value = undefined
      if (isRequestCancelled(cause)) {
        cancelled.value = true
        return
      }
      error.value = isUncertainWrite(cause) ? '操作结果尚未确认，请先刷新检查，避免重复提交。'
        : cause instanceof Error ? cause.message
          : cause && typeof cause === 'object' && 'msg' in cause ? String(cause.msg) : '上传失败'
    }).finally(() => {
      if (current === controller) {
        current = undefined
        active.value = 0
      }
    })
  }

  function cancel() {
    if (!current)
      return
    current.abort()
    current = undefined
    active.value = 0
    progress.value = undefined
    error.value = ''
    cancelled.value = true
  }

  onBeforeUnmount(() => {
    alive = false
    current?.abort()
    current = undefined
    active.value = 0
    progress.value = undefined
  })
  return { active, error, cancelled, progress, upload, cancel }
}
