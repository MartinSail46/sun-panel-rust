<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, ImageOff } from '@lucide/vue'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useDialogFocus } from '@/hooks/useDialogFocus'

const props = withDefaults(defineProps<{
  open: boolean
  images: { src: string, alt?: string }[]
  index?: number
}>(), { index: 0 })
const emit = defineEmits<{ 'update:open': [value: boolean] }>()
const { ownerId, onCloseAutoFocus } = useDialogFocus(() => props.open)
const selected = ref(0)
const failed = ref(false)
const loading = ref(true)
const current = computed(() => props.images[selected.value])
const text = { title: '图片预览', previous: '上一张', next: '下一张', help: '使用左右方向键切换，Esc 关闭', failed: '图片加载失败', loading: '图片加载中' }

function clampIndex(index: number) {
  return Math.max(0, Math.min(Number.isFinite(index) ? Math.trunc(index) : 0, props.images.length - 1))
}

watch(() => [props.open, props.index] as const, ([open, index]) => {
  if (open) {
    selected.value = clampIndex(index)
    failed.value = false
    loading.value = true
  }
}, { immediate: true })
watch(() => props.images.length, (length) => {
  selected.value = clampIndex(selected.value)
  if (!length && props.open) emit('update:open', false)
})
watch(() => current.value?.src, () => {
  failed.value = false
  loading.value = true
}, { immediate: true })

function move(direction: number) {
  if (props.images.length < 2) return
  selected.value = (selected.value + direction + props.images.length) % props.images.length
}
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogContent :data-dialog-focus-owner="ownerId" close-label="关闭" class="max-h-[calc(100dvh-2rem)] overflow-auto sm:max-w-5xl" @keydown.left.prevent.stop="move(-1)" @keydown.right.prevent.stop="move(1)" @close-auto-focus="onCloseAutoFocus">
      <DialogHeader class="pr-8">
        <DialogTitle class="truncate">
          {{ current?.alt || text.title }}
        </DialogTitle>
        <DialogDescription>{{ selected + 1 }} / {{ images.length }} · {{ text.help }}</DialogDescription>
      </DialogHeader>
      <div class="relative flex min-h-48 items-center justify-center rounded-md border bg-muted/40">
        <p v-if="loading && !failed && current" role="status" class="absolute text-sm text-muted-foreground">
          {{ text.loading }}
        </p>
        <div v-if="failed || !current" role="alert" class="flex flex-col items-center gap-3 px-4 py-16 text-sm text-muted-foreground">
          <ImageOff class="size-8" />
          <span>{{ text.failed }}</span>
        </div>
        <img v-else :key="current.src" :src="current.src" :alt="current.alt || text.title" class="relative max-h-[65dvh] max-w-full object-contain" @load="loading = false" @error="failed = true; loading = false">
      </div>
      <div v-if="images.length > 1" class="flex justify-center gap-3">
        <Button variant="outline" size="sm" @click="move(-1)">
          <ChevronLeft />{{ text.previous }}
        </Button>
        <Button variant="outline" size="sm" @click="move(1)">
          {{ text.next }}<ChevronRight />
        </Button>
      </div>
    </DialogContent>
  </Dialog>
</template>
