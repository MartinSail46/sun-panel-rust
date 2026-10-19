<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import AppSelect from '@/components/common/AppSelect.vue'
import { toast } from '@/lib/feedback'
import { RoundCardModal } from '@/components/common'
import { exportNavigation, importNavigation, previewNavigation } from '@/api/panel/navigation'
import { useNavigationStore } from '@/store/modules/navigation'
import { isRequestCancelled, isUncertainWrite, RequestError } from '@/utils/requestLifetime'

interface Preview {
  mode: string
  revision: number
  groups: number
  items: number
  newGroups: number
  updatedGroups: number
  duplicateGroups: number
  newItems: number
  updatedItems: number
  duplicateItems: number
  removedGroups: number
  removedItems: number
  conflicts: string[]
}
const fileInput = ref<HTMLInputElement>()
const text = {
  title: '导航迁移', info: '导航文件包含分组、图标引用和应用地址，不包含图片文件、外观或小组件设置。',
  import: '选择导航文件', export: '导出导航', preview: '导入预览', append: '追加（保留重复项）', merge: '合并（相同分组与地址不重复创建）', replace: '替换全部导航',
  totals: '分组 / 应用', added: '新增', updated: '更新', duplicate: '重复（跳过）', removed: '替换时删除',
  cancel: '取消', confirm: '确认导入', replaceConfirm: '输入“替换全部导航”确认', replacePhrase: '替换全部导航', loading: '正在检查文件…',
  invalid: '文件不是支持的导航格式，或内容不完整。', size: '请选择小于 8 MiB 的导航文件。',
  success: '导航导入完成，首页已更新。', backup: '完整实例备份', backupTip: '完整备份包含数据库、外观、小组件和上传图片。请先停止服务，再在服务器执行维护命令。恢复应使用新的空实例。',
  native: '本机部署（在项目目录执行）', docker: 'Docker 部署', source: '恢复到新目录', copied: '命令已复制', copy: '复制命令', error: '操作失败，请重试。',
}
const mode = ref<'merge' | 'append' | 'replace'>('merge')
const options = [{ label: text.merge, value: 'merge' }, { label: text.append, value: 'append' }, { label: text.replace, value: 'replace' }]
const document = ref<Record<string, unknown>>()
const stats = ref<Preview>()
const modal = ref(false)
const loading = ref(false)
const submitting = ref(false)
const exporting = ref(false)
const error = ref('')
const confirmation = ref('')
let generation = 0
let previewController: AbortController | undefined
let exportController: AbortController | undefined
let fileGeneration = 0
const refreshWarning = '导航已导入，但首页刷新失败。请刷新页面，不要重复导入。'
function setModal(value: boolean) { if (!submitting.value) modal.value = value }
watch(modal, (visible) => {
  if (!visible) { previewController?.abort(); generation++; loading.value = false }
})
onBeforeUnmount(() => {
  fileGeneration++
  generation++
  previewController?.abort()
  exportController?.abort()
})

function message(cause: unknown) {
  if (isUncertainWrite(cause)) return '操作结果尚未确认，请先刷新检查，避免重复提交。'
  if (cause instanceof RequestError && cause.kind === 'timeout') return '请求超时，请稍后重试'
  if (cause instanceof Error) return cause.message
  if (cause && typeof cause === 'object' && 'msg' in cause) return String(cause.msg)
  return text.error
}
async function updatePreview() {
  previewController?.abort()
  const controller = new AbortController()
  previewController = controller
  const current = ++generation
  const value = document.value
  if (!value) return
  stats.value = undefined
  confirmation.value = ''
  error.value = ''
  loading.value = true
  try {
    const response = await previewNavigation<Preview>({ ...value, mode: mode.value }, { signal: controller.signal })
    if (current !== generation) return
    if (response.code !== 0) throw response
    stats.value = response.data
  }
  catch (cause) {
    if (current === generation && !isRequestCancelled(cause)) error.value = message(cause)
  }
  finally { if (current === generation) loading.value = false }
}
async function chooseFile(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  const selected = ++fileGeneration
  if (file.size > 8 * 1024 * 1024) { toast.error(text.size); return }
  try {
    const raw = await file.text()
    if (selected !== fileGeneration) return
    const value = JSON.parse(raw)
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(text.invalid)
    if (value.appName !== 'Sun-Panel-Navigation' || value.schemaVersion !== 1) throw new Error(text.invalid)
    document.value = value
    mode.value = 'merge'
    modal.value = true
    await updatePreview()
  }
  catch (cause) { if (selected === fileGeneration) toast.error(message(cause)) }
}
function changeMode(value: string | number) {
  if (value !== 'merge' && value !== 'append' && value !== 'replace') return
  mode.value = value
  void updatePreview()
}

async function startImport() {
  if (!document.value || !stats.value || loading.value || submitting.value) return
  if (stats.value.conflicts.length || (mode.value === 'replace' && confirmation.value !== text.replacePhrase)) return
  submitting.value = true
  error.value = ''
  try {
    const response = await importNavigation({ ...document.value, mode: mode.value, revision: stats.value.revision })
    if (response.code !== 0) throw response
    modal.value = false
    try {
      await useNavigationStore().refresh()
      toast.success(text.success)
    }
    catch { toast.warning(refreshWarning) }
  }
  catch (cause) {
    error.value = message(cause)
    stats.value = undefined // Never automatically replay an unconfirmed import.
    if (isUncertainWrite(cause)) {
      try { await useNavigationStore().refresh() }
      catch { /* Keep the uncertain-result message and require a fresh preview. */ }
    }
  }
  finally { submitting.value = false }
}
async function startExport() {
  exporting.value = true
  exportController?.abort()
  exportController = new AbortController()
  try {
    const response = await exportNavigation<Record<string, unknown>>({ signal: exportController.signal })
    if (response.code !== 0) throw response
    const url = URL.createObjectURL(new Blob([JSON.stringify(response.data, null, 2)], { type: 'application/json' }))
    try {
      const anchor = globalThis.document.createElement('a')
      anchor.href = url
      anchor.download = `SunPanel-Navigation-${new Date().toISOString().slice(0, 10)}.sunpanel.json`
      anchor.click()
    }
    finally { setTimeout(() => URL.revokeObjectURL(url), 0) }
  }
  catch (cause) { if (!isRequestCancelled(cause)) toast.error(message(cause)) }
  finally { exporting.value = false; exportController = undefined }
}
const nativeCommands = 'SUN_PANEL_DATA_DIR=./data ./service-rust/target/release/sun-panel-rust backup ./instance-backups/manual-001\nSUN_PANEL_DATA_DIR=./restored-data ./service-rust/target/release/sun-panel-rust restore ./instance-backups/manual-001'
const dockerCommands = 'docker compose stop sun-panel-rust\ndocker compose run --rm --no-deps sun-panel-rust /app/sun-panel-rust backup /app/data/backups/manual-001\nmkdir -p ./instance-backups\ndocker compose cp sun-panel-rust:/app/data/backups/manual-001 ./instance-backups/manual-001\ndocker compose start sun-panel-rust'
async function copy(value: string) {
  try { await navigator.clipboard.writeText(value); toast.success(text.copied) }
  catch { toast.error(text.error) }
}
</script>

<template>
  <div class="space-y-4 p-4">
    <Card>
      <CardHeader><CardTitle>{{ text.title }}</CardTitle></CardHeader>
      <CardContent class="space-y-4">
        <p class="rounded-lg border bg-muted/50 p-3 text-sm text-muted-foreground">
          {{ text.info }}
        </p>
        <div class="flex flex-wrap gap-2">
          <input ref="fileInput" type="file" accept=".json,.sunpanel.json" class="hidden" :disabled="submitting || loading" @change="chooseFile">
          <Button :disabled="submitting || loading" @click="fileInput?.click()">
            {{ text.import }}
          </Button>
          <Button variant="outline" :disabled="exporting" @click="startExport">
            {{ text.export }}
          </Button>
        </div>
      </CardContent>
    </Card>
    <Card>
      <CardHeader><CardTitle>{{ text.backup }}</CardTitle></CardHeader>
      <CardContent class="space-y-4">
        <p class="text-sm text-muted-foreground">
          {{ text.backupTip }}
        </p>
        <details class="rounded-lg border p-3">
          <summary class="cursor-pointer text-sm font-medium">
            {{ text.native }}
          </summary>
          <pre class="my-3 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-3 text-xs">{{ nativeCommands }}</pre>
          <Button variant="outline" size="sm" @click="copy(nativeCommands)">
            {{ text.copy }}
          </Button>
        </details>
        <details class="rounded-lg border p-3">
          <summary class="cursor-pointer text-sm font-medium">
            {{ text.docker }}
          </summary>
          <pre class="my-3 overflow-auto whitespace-pre-wrap break-all rounded-md bg-muted p-3 text-xs">{{ dockerCommands }}</pre>
          <Button variant="outline" size="sm" @click="copy(dockerCommands)">
            {{ text.copy }}
          </Button>
        </details>
      </CardContent>
    </Card>
    <RoundCardModal :show="modal" :closable="!submitting" :mask-closable="!submitting" :close-on-esc="!submitting" :title="text.preview" class="max-w-xl" @update:show="setModal">
      <div class="space-y-4">
        <AppSelect :model-value="mode" :options="options" :disabled="submitting" :aria-label="text.preview" @update:model-value="changeMode" />
        <p v-if="loading" role="status" class="text-sm text-muted-foreground">
          {{ text.loading }}
        </p>
        <p v-if="error" role="alert" class="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
          {{ error }}
        </p>
        <dl v-if="stats" class="grid grid-cols-2 gap-x-3 gap-y-2 text-sm">
          <dt class="text-muted-foreground">
            {{ text.totals }}
          </dt><dd class="text-right tabular-nums">
            {{ stats.groups }} / {{ stats.items }}
          </dd>
          <dt class="text-muted-foreground">
            {{ text.added }}
          </dt><dd class="text-right tabular-nums">
            {{ stats.newGroups }} / {{ stats.newItems }}
          </dd>
          <dt class="text-muted-foreground">
            {{ text.updated }}
          </dt><dd class="text-right tabular-nums">
            {{ stats.updatedGroups }} / {{ stats.updatedItems }}
          </dd>
          <dt class="text-muted-foreground">
            {{ text.duplicate }}
          </dt><dd class="text-right tabular-nums">
            {{ stats.duplicateGroups }} / {{ stats.duplicateItems }}
          </dd>
          <template v-if="mode === 'replace'">
            <dt class="text-muted-foreground">
              {{ text.removed }}
            </dt><dd class="text-right tabular-nums">
              {{ stats.removedGroups }} / {{ stats.removedItems }}
            </dd>
          </template>
        </dl>
        <template v-if="stats">
          <p v-for="conflict in stats.conflicts" :key="conflict" role="alert" class="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
            {{ conflict }}
          </p>
          <Input v-if="mode === 'replace'" v-model="confirmation" :placeholder="text.replaceConfirm" :aria-label="text.replaceConfirm" :disabled="submitting" />
        </template>
        <div class="flex flex-wrap justify-end gap-2">
          <Button v-if="!stats && !loading" variant="outline" :disabled="submitting" @click="updatePreview">
            {{ text.preview }}
          </Button>
          <Button variant="outline" :disabled="submitting" @click="setModal(false)">
            {{ text.cancel }}
          </Button>
          <Button :disabled="submitting || loading || !stats || !!stats.conflicts.length || (mode === 'replace' && confirmation !== text.replacePhrase)" @click="startImport">
            {{ text.confirm }}
          </Button>
        </div>
      </div>
    </RoundCardModal>
  </div>
</template>
