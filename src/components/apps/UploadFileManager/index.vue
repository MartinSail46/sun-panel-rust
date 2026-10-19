<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import ImagePreview from '@/components/common/ImagePreview.vue'
import { toast, confirmDialog } from '@/lib/feedback'
import { onMounted, ref } from 'vue'
import { deletes, getList } from '@/api/system/file'
import { RoundCardModal, SvgIcon } from '@/components/common'
import { copyToClipboard, timeFormat } from '@/utils/cmn'
import { usePanelState } from '@/store'

interface InfoModalState {
  title: string
  show: boolean
  fileInfo: File.Info | null
}
const imageList = ref<File.Info[]>([])
const previewOpen = ref(false)
const previewIndex = ref(0)
const panelStore = usePanelState()
const loading = ref(false)
const wallpaperPending = ref<string | null>(null)
const infoModalState = ref<InfoModalState>({
  show: false,
  title: '',
  fileInfo: null,
})

async function getFileList() {
  loading.value = true
  try {
    const { code, data } = await getList<Common.ListResponse<File.Info[]>>()
    if (code === 0) {
      imageList.value = data.list
      if (infoModalState.value.fileInfo)
        infoModalState.value.fileInfo = imageList.value.find(file => file.id === infoModalState.value.fileInfo?.id) || null
    }
  }
  catch {
    toast.error('操作失败')
  }
  finally {
    loading.value = false
  }
}

async function copyImageUrl(text: string) {
  const res = await copyToClipboard(text)
  if (res)
    toast.success('链接复制成功，可以在图标地址栏')

  else
    toast.error('复制失败')
}

async function handleDelete(file: File.Info) {
  if (file.referenceCount) {
    handleInfoClick(file)
    toast.warning('图片正在使用，请先替换引用后再删除。')
    return
  }
  if (await confirmDialog({
    title: '警告',
    description: '删除后无法恢复，你确定要继续吗？',
    confirmText: '确定',
    cancelText: '取消',
    destructive: true,
  })) await deletesImges(file.id as number)
}

async function deletesImges(id: number) {
  try {
    const { code, msg } = await deletes([id])
    if (code === 0) {
      await getFileList()
      toast.success('操作成功')
    }
    else {
      await getFileList()
      if (code === 1310) {
        const current = imageList.value.find(file => file.id === id)
        if (current)
          handleInfoClick(current)
      }
      else {
        toast.error(`${'操作失败'}:${msg}`)
      }
      return false
    }
  }
  catch (error) {
    // A previously unused image may have been assigned in another tab while
    // the confirmation dialog was open. Refresh its server-owned references.
    await getFileList()
    toast.error(typeof error === 'object' && error !== null && 'code' in error && error.code === 1310 ? '图片正在使用，请先替换引用后再删除。' : '操作失败')
    return false
  }
}

function handleInfoClick(fileInfo: File.Info) {
  infoModalState.value.fileInfo = fileInfo
  infoModalState.value.show = true
}

async function handleSetWallpaper(imgSrc: string) {
  if (wallpaperPending.value)
    return
  wallpaperPending.value = imgSrc
  try {
    if (await panelStore.savePanelChanges({ backgroundImageSrc: imgSrc })) {
      toast.success('操作成功')
      await getFileList()
    }
    else {
      toast.error('保存失败')
    }
  }
  catch {
    toast.error('保存失败')
  }
  finally {
    wallpaperPending.value = null
  }
}

function referenceLabel(reference: File.Reference) {
  const title = reference.title || `#${reference.recordId}`
  const label = { wallpaper: '面板壁纸', navigation: `导航图标：${title}`, group: `分组图标：${title}`, footer: '页脚内容' }[reference.type]
  return reference.inTrash ? `${label} · 回收站` : label
}

function previewImage(index: number) {
  previewIndex.value = index
  previewOpen.value = true
}

onMounted(() => {
  getFileList()
})
</script>

<template>
  <div class="space-y-4 p-4">
    <div class="flex items-center justify-end gap-2">
      <Button variant="outline" size="sm" :disabled="loading" @click="getFileList">
        刷新
      </Button>
    </div>
    <p class="rounded-lg border bg-muted/50 p-3 text-sm text-muted-foreground">
      你可以在这里管理你上传过的壁纸和图标
    </p>
    <p v-if="imageList.length === 0 && !loading" class="py-10 text-center text-sm text-muted-foreground">
      你还没有上传过任何图片
    </p>
    <div v-else class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-6">
      <Card v-for="(item, index) in imageList" :key="item.id" class="gap-0 overflow-hidden py-0">
        <Button type="button" variant="ghost" class="transparent-grid h-28 w-full overflow-hidden rounded-none border-b p-0" :aria-label="item.fileName" @click="previewImage(index)">
          <img loading="lazy" class="h-full w-full object-contain" :src="item.src" :alt="item.fileName">
        </Button>
        <CardContent class="space-y-2 p-3">
          <p class="truncate text-xs" :title="item.fileName">
            {{ item.fileName }}
          </p>
          <Button type="button" variant="link" class="h-auto justify-start p-0 text-left text-xs text-muted-foreground underline underline-offset-2" @click="handleInfoClick(item)">
            {{ item.referenceCount ? `使用中（${item.referenceCount}处）` : '未被使用' }}
          </Button>
          <div class="flex flex-wrap gap-1">
            <Button variant="ghost" size="icon" class="size-7" title="复制链接" aria-label="复制链接" @click="copyImageUrl(item.src)">
              <SvgIcon icon="ion-copy" />
            </Button>
            <Button variant="ghost" size="icon" class="size-7" :title="timeFormat(item.createTime)" aria-label="文件详情" @click="handleInfoClick(item)">
              <SvgIcon icon="mdi-information-box-outline" />
            </Button>
            <Button variant="ghost" size="icon" class="size-7" title="设置为壁纸" aria-label="设置为壁纸" :disabled="wallpaperPending !== null" @click="handleSetWallpaper(item.src)">
              <SvgIcon icon="lucide:wallpaper" />
            </Button>
            <Button variant="ghost" size="icon" class="size-7 text-destructive hover:text-destructive" :title="item.referenceCount ? '图片正在使用，请先替换引用后再删除。' : '删除'" aria-label="删除" @click="handleDelete(item)">
              <SvgIcon icon="material-symbols-delete" />
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
    <ImagePreview v-model:open="previewOpen" :images="imageList.map(item => ({ src: item.src, alt: item.fileName }))" :index="previewIndex" />
    <RoundCardModal v-model:show="infoModalState.show" class="max-w-sm" title="文件详情">
      <dl class="space-y-4 break-words text-sm">
        <div>
          <dt class="mb-1 font-medium">
            图片用途
          </dt>
          <dd>
            <ul v-if="infoModalState.fileInfo?.references?.length" class="space-y-1 text-xs text-muted-foreground">
              <li v-for="(reference, index) in infoModalState.fileInfo.references" :key="index">
                {{ referenceLabel(reference) }}
              </li>
            </ul>
            <p v-else class="text-xs text-muted-foreground">
              未被使用
            </p>
            <p v-if="infoModalState.fileInfo?.referenceCount" class="mt-2 rounded-md border border-amber-500/30 bg-amber-500/10 p-2 text-xs text-amber-800 dark:text-amber-200">
              请到下列位置替换图片，刷新用途后再删除。
            </p>
          </dd>
        </div>
        <div>
          <dt class="mb-1 font-medium">
            原文件名
          </dt><dd class="text-xs text-muted-foreground">
            {{ infoModalState.fileInfo?.fileName }}
          </dd>
        </div>
        <div>
          <dt class="mb-1 font-medium">
            文件路径
          </dt><dd class="break-all text-xs text-muted-foreground">
            {{ infoModalState.fileInfo?.src }}
          </dd>
        </div>
        <div>
          <dt class="mb-1 font-medium">
            上传时间
          </dt><dd class="text-xs text-muted-foreground">
            {{ timeFormat(infoModalState.fileInfo?.createTime) }}
          </dd>
        </div>
      </dl>
    </RoundCardModal>
  </div>
</template>

<style scoped>
.transparent-grid {
  background-color: var(--muted);
  background-image: linear-gradient(45deg, #8882 25%, transparent 25%, transparent 75%, #8882 75%),
    linear-gradient(45deg, #8882 25%, transparent 25%, transparent 75%, #8882 75%);
  background-size: 16px 16px;
  background-position: 0 0, 8px 8px;
}
</style>
