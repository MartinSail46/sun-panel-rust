<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { CircleAlert, CircleCheck, Clock, Info, Loader2 } from '@lucide/vue'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Spinner } from '@/components/ui/spinner'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { Switch } from '@/components/ui/switch'
import AppSelect from '@/components/common/AppSelect.vue'
import AppSlider from '@/components/common/AppSlider.vue'
import ImageUpload from '@/components/common/ImageUpload.vue'
import { confirmDialog } from '@/lib/feedback'
import { usePanelState } from '@/store'
import type { SaveState } from '@/utils/settingsSave'
import { useImageUpload } from '@/hooks/useImageUpload'
import { DEFAULT_WALLPAPER } from '@/utils/wallpaper'

const panelState = usePanelState()
const showWallpaperInput = ref(false)
const { active: uploads, progress: uploadProgress, error: uploadError, cancelled: uploadCancelled, upload, cancel: cancelUpload } = useImageUpload(data => { void panelState.savePanelChanges({ backgroundImageSrc: data.imageUrl }) })

const saveState = ref<SaveState>({ status: 'idle' })
const unsubscribe = panelState.subscribePanelSave(state => saveState.value = state)
const saveStatusText = computed(() => {
  switch (saveState.value.status) {
    case 'pending': return '等待保存…'
    case 'saving': return '正在保存…'
    case 'saved': return '配置已保存'
    case 'error': return `配置保存失败：${saveState.value.error}`
    default: return '修改后自动保存'
  }
})
const saveStatusIcon = computed(() => ({
  idle: Info,
  pending: Clock,
  saving: Loader2,
  saved: CircleCheck,
  error: CircleAlert,
})[saveState.value.status])
const saveStatusTone = computed(() => saveState.value.status === 'error'
  ? 'text-destructive'
  : saveState.value.status === 'saved' || saveState.value.status === 'saving'
    ? 'text-primary'
    : 'text-muted-foreground')

onBeforeUnmount(() => {
  unsubscribe()
  // Vue cannot await unmount hooks; the shared queue owns and finishes the request.
  void panelState.flushPanelSave()
})

const maxWidthUnitOption = [
  {
    label: 'px',
    value: 'px',
  },
  {
    label: '%',
    value: '%',
  },
]

watch(() => panelState.panelConfig, () => {
  // Record locally before scheduling any network writes.
  panelState.recordState()
  panelState.schedulePanelSave(panelState.panelConfig)
}, { deep: true, flush: 'sync' })

function retrySave() {
  panelState.recordState()
  panelState.schedulePanelSave(panelState.panelConfig)
  return panelState.flushPanelSave()
}

async function resetPanelConfig() {
  if (await confirmDialog({ title: '重置', description: '确定要将页面风格恢复为默认设置吗？', destructive: true }))
    await panelState.resetPanelConfig()
}

async function reloadPanelConfig() {
  if (await confirmDialog({ title: '载入服务器配置', description: '放弃当前未保存的修改，并载入服务器最新配置？', destructive: true }))
    await panelState.reloadPanelConfig()
}

function setMaxWidth(value: string | number) {
  const width = Number(value)
  panelState.panelConfig.maxWidth = value === '' || !Number.isFinite(width) ? undefined : Math.max(0, width)
}

</script>

<template>
  <div class="flex h-full min-h-0 flex-col">
    <header class="flex shrink-0 flex-wrap items-center justify-between gap-x-3 gap-y-2 border-b px-3 py-3 sm:px-4">
      <p class="flex min-w-0 items-start gap-2 text-xs leading-5" :class="[saveStatusTone, { 'basis-full sm:basis-auto': saveState.status === 'error' }]" role="status" aria-live="polite" :aria-busy="saveState.status === 'saving'">
        <component :is="saveStatusIcon" class="mt-0.5 size-4 shrink-0" :class="{ 'animate-spin motion-reduce:animate-none': saveState.status === 'saving' }" aria-hidden="true" />
        <span class="break-words">{{ saveStatusText }}</span>
      </p>
      <div v-if="saveState.status === 'error'" class="ml-auto flex shrink-0 flex-wrap gap-2">
        <Button variant="outline" size="sm" @click="reloadPanelConfig">
          载入服务器配置
        </Button>
        <Button type="button" variant="outline" size="sm" @click="retrySave">
          重试
        </Button>
      </div>
    </header>
    <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
      <div class="grid gap-3 p-3 sm:p-4">
        <Card class="py-3">
          <CardContent class="grid gap-3 px-4">
            <h2 class="font-semibold">
              LOGO
            </h2>
            <div class="grid gap-2">
              <div class="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                <label for="panel-logo-text" class="text-sm">文本内容</label>
                <span class="tabular-nums">{{ panelState.panelConfig.logoText?.length || 0 }} / 20</span>
              </div>
              <Input id="panel-logo-text" v-model="panelState.panelConfig.logoText" :maxlength="20" />
            </div>
          </CardContent>
        </Card>
        <Card class="py-3">
          <CardContent class="px-4">
            <h2 class="mb-1 font-semibold">
              显示设置
            </h2>
            <div class="divide-y divide-border">
              <div class="flex min-h-11 items-center justify-between gap-4 py-2 text-sm">
                <label for="clock-seconds">时钟显示秒数</label>
                <Switch id="clock-seconds" v-model="panelState.panelConfig.clockShowSecond" />
              </div>
              <div class="flex min-h-11 items-center justify-between gap-4 py-2 text-sm">
                <label for="search-visible">显示搜索栏</label>
                <Switch id="search-visible" v-model="panelState.panelConfig.searchBoxShow" />
              </div>
              <div class="flex min-h-11 items-center justify-between gap-4 py-2 text-sm">
                <label for="monitor-visible">显示系统状态</label>
                <Switch id="monitor-visible" v-model="panelState.panelConfig.systemMonitorShow" />
              </div>
              <div class="flex min-h-11 items-center justify-between gap-4 py-2 text-sm" :class="{ 'text-muted-foreground': !panelState.panelConfig.systemMonitorShow }">
                <label for="monitor-title-visible">显示系统状态标题</label>
                <Switch id="monitor-title-visible" v-model="panelState.panelConfig.systemMonitorShowTitle" :disabled="!panelState.panelConfig.systemMonitorShow" />
              </div>
            </div>
          </CardContent>
        </Card>
        <Card class="py-3">
          <CardContent class="grid gap-3 px-4">
            <h2 class="font-semibold">
              壁纸
            </h2>
            <ImageUpload @select="upload">
              <div
                class="flex h-36 w-full items-center justify-center rounded-lg border bg-muted bg-cover bg-center p-4 text-center"
                :style="{ backgroundImage: `url(${panelState.panelConfig.backgroundImageSrc})` }"
              >
                <span class="rounded-md bg-black/50 px-3 py-2 text-sm text-white">点击上传替换图片或拖拽到框内</span>
              </div>
            </ImageUpload>
            <div class="flex flex-wrap gap-2">
              <Button variant="outline" size="sm" @click="panelState.savePanelChanges({ backgroundImageSrc: DEFAULT_WALLPAPER })">
                使用默认壁纸
              </Button>
              <Button v-if="uploads" variant="outline" size="sm" @click="cancelUpload">
                取消上传
              </Button>
            </div>
            <Progress v-if="uploads && uploadProgress !== undefined" :model-value="uploadProgress" class="h-1.5" aria-label="上传中" />
            <Spinner v-else-if="uploads" aria-label="上传中" />
            <p v-if="uploadError" role="alert" class="text-xs text-destructive">
              {{ uploadError }}
            </p>
            <p v-else-if="uploadCancelled" role="status" class="text-xs text-muted-foreground">
              上传已取消；若文件已送达服务器，可在文件管理中查看。
            </p>
            <div class="flex min-h-11 items-center justify-between gap-4 text-sm">
              <label for="custom-wallpaper">使用图片链接</label>
              <Switch id="custom-wallpaper" v-model="showWallpaperInput" />
            </div>
            <Input v-if="showWallpaperInput" v-model="panelState.panelConfig.backgroundImageSrc" aria-label="壁纸图片链接" />
            <div class="grid gap-3 sm:grid-cols-2 sm:gap-4">
              <div class="grid gap-2">
                <label class="text-sm text-muted-foreground">模糊</label>
                <AppSlider v-model="panelState.panelConfig.backgroundBlur" :step="2" :max="20" aria-label="模糊" />
              </div>
              <div class="grid gap-2">
                <label class="text-sm text-muted-foreground">遮罩</label>
                <AppSlider v-model="panelState.panelConfig.backgroundMaskNumber" :step="0.1" :max="1" aria-label="遮罩" />
              </div>
            </div>
          </CardContent>
        </Card>
        <Card class="py-3">
          <CardContent class="grid gap-3 px-4">
            <h2 class="font-semibold">
              内容区域
            </h2>
            <div class="grid gap-2">
              <label for="content-max-width" class="text-sm text-muted-foreground">最大宽度</label>
              <div class="flex max-w-sm gap-2">
                <Input id="content-max-width" :model-value="panelState.panelConfig.maxWidth ?? ''" type="number" min="0" class="min-w-0 flex-1" placeholder="1200" @update:model-value="setMaxWidth" />
                <AppSelect v-model="panelState.panelConfig.maxWidthUnit" class="w-20 shrink-0" :options="maxWidthUnitOption" aria-label="最大宽度单位" />
              </div>
            </div>
            <div class="grid gap-2">
              <label class="text-sm text-muted-foreground">左右边距 (px)</label>
              <AppSlider v-model="panelState.panelConfig.marginX" :max="100" aria-label="左右边距" />
            </div>
            <div class="grid gap-3 sm:grid-cols-2 sm:gap-4">
              <div class="grid gap-2">
                <label class="text-sm text-muted-foreground">上边距 (屏高 %)</label>
                <AppSlider v-model="panelState.panelConfig.marginTop" :max="50" aria-label="上边距" />
              </div>
              <div class="grid gap-2">
                <label class="text-sm text-muted-foreground">下边距 (屏高 %)</label>
                <AppSlider v-model="panelState.panelConfig.marginBottom" :max="50" aria-label="下边距" />
              </div>
            </div>
          </CardContent>
        </Card>
        <Card class="py-3">
          <CardContent class="grid gap-3 px-4">
            <h2 class="font-semibold">
              自定义页脚
            </h2>
            <Textarea v-model="panelState.panelConfig.footerHtml" aria-label="自定义页脚" rows="3" />
          </CardContent>
        </Card>
      </div>
    </div>
    <footer class="flex shrink-0 items-center justify-between gap-3 border-t px-3 py-3 sm:px-4">
      <p class="text-xs text-muted-foreground">
        将页面风格恢复为默认设置。
      </p>
      <Button variant="outline" size="sm" class="shrink-0 text-destructive hover:text-destructive" @click="resetPanelConfig">
        重置页面风格
      </Button>
    </footer>
  </div>
</template>
