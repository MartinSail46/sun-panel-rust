<script setup lang="ts">
import { computed, useId } from 'vue'
import { ItemIcon } from '@/components/common'
import ColorPicker from '@/components/common/ColorPicker.vue'
import ImageUpload from '@/components/common/ImageUpload.vue'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { Spinner } from '@/components/ui/spinner'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { useImageUpload } from '@/hooks/useImageUpload'

const props = withDefaults(defineProps<{
  itemIcon: Panel.ItemIcon | null
  disabled?: boolean
}>(), { disabled: false })
const emit = defineEmits<{
  (e: 'update:itemIcon', icon: Panel.ItemIcon): void
}>()
const id = useId()
const backgroundSwatches = ['#00000000', '#000000', '#ffffff', '#18A058', '#2080F0', '#F0A020', 'rgba(208, 48, 80, 1)', '#C418D1FF']
const defaultBackground = '#2a2a2a6b'
const itemIconInfo = computed<Panel.ItemIcon>(() => ({
  itemType: 2,
  ...props.itemIcon,
  backgroundColor: props.itemIcon?.backgroundColor || defaultBackground,
}))
function update(patch: Partial<Panel.ItemIcon>) {
  emit('update:itemIcon', { ...itemIconInfo.value, ...patch })
}
const iconText = computed({ get: () => itemIconInfo.value.text || '', set: text => update({ text }) })
const iconSrc = computed({ get: () => itemIconInfo.value.src || '', set: src => update({ src }) })
const backgroundColor = computed({ get: () => itemIconInfo.value.backgroundColor || defaultBackground, set: value => update({ backgroundColor: value }) })
const { active: uploads, progress: uploadProgress, error: uploadError, cancelled: uploadCancelled, upload, cancel: cancelUpload } = useImageUpload(data => update({ src: data.imageUrl }))
</script>

<template>
  <div class="w-full space-y-4 rounded-xl border border-border bg-card p-3 text-card-foreground">
    <RadioGroup :model-value="String(itemIconInfo.itemType)" :disabled="disabled" class="flex flex-wrap gap-x-4 gap-y-2" aria-label="图标" @update:model-value="value => update({ itemType: Number(value) })">
      <div class="flex items-center gap-2">
        <RadioGroupItem :id="`${id}-text`" value="1" />
        <Label :for="`${id}-text`" class="cursor-pointer">文字</Label>
      </div>
      <div class="flex items-center gap-2">
        <RadioGroupItem :id="`${id}-image`" value="2" />
        <Label :for="`${id}-image`" class="cursor-pointer">图片</Label>
      </div>
      <div class="flex items-center gap-2">
        <RadioGroupItem :id="`${id}-online`" value="3" />
        <Label :for="`${id}-online`" class="cursor-pointer">在线图标</Label>
      </div>
    </RadioGroup>
    <div class="flex items-start gap-4">
      <div class="transparent-grid shrink-0 overflow-hidden rounded-2xl border border-border bg-muted">
        <ItemIcon :item-icon="itemIconInfo" />
      </div>
      <div class="min-w-0 flex-1 space-y-2">
        <template v-if="itemIconInfo.itemType === 1">
          <Label :for="`${id}-text-value`" class="sr-only">文字</Label>
          <Input :id="`${id}-text-value`" v-model="iconText" :disabled="disabled" aria-label="文字" />
        </template>
        <template v-else-if="itemIconInfo.itemType === 3">
          <Label :for="`${id}-online-name`" class="sr-only">请输入图标名称</Label>
          <Input :id="`${id}-online-name`" v-model="iconText" :disabled="disabled" placeholder="请输入图标名称" />
          <Button variant="link" size="sm" class="h-auto p-0" as-child>
            <a target="_blank" rel="noopener noreferrer" href="https://icon-sets.iconify.design/">在线图标库</a>
          </Button>
        </template>
        <template v-else>
          <Label :for="`${id}-image-url`" class="sr-only">输入图标地址或上传</Label>
          <Input :id="`${id}-image-url`" v-model="iconSrc" :disabled="disabled" placeholder="输入图标地址或上传" />
          <div class="flex flex-wrap gap-2">
            <ImageUpload :disabled="disabled" @select="upload">
              <span class="block px-3 py-2 text-sm">
                本地上传
              </span>
            </ImageUpload>
            <Button v-if="uploads" type="button" variant="outline" size="sm" @click="cancelUpload">
              取消上传
            </Button>
          </div>
          <Progress v-if="uploads && uploadProgress !== undefined" :model-value="uploadProgress" class="mt-2 h-1.5" aria-label="上传中" />
          <Spinner v-else-if="uploads" aria-label="上传中" />
          <p v-if="uploadError" role="alert" class="text-xs text-destructive">
            {{ uploadError }}
          </p>
          <p v-else-if="uploadCancelled" role="status" class="text-xs text-muted-foreground">
            上传已取消；若文件已送达服务器，可在文件管理中查看。
          </p>
        </template>
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-3">
      <Label :for="`${id}-background`" class="shrink-0 text-muted-foreground">背景颜色</Label>
      <ColorPicker :id="`${id}-background`" v-model="backgroundColor" class="w-[150px]" :swatches="backgroundSwatches" :disabled="disabled" />
      <Button v-if="backgroundColor !== defaultBackground" type="button" variant="ghost" size="sm" :disabled="disabled" @click="backgroundColor = defaultBackground">
        重置
      </Button>
    </div>
  </div>
</template>

<style scoped>
.transparent-grid {
  background-image: linear-gradient(45deg, #a1a1aa33 25%, transparent 25%, transparent 75%, #a1a1aa33 75%),
                    linear-gradient(45deg, #a1a1aa33 25%, transparent 25%, transparent 75%, #a1a1aa33 75%);
  background-size: 16px 16px;
  background-position: 0 0, 8px 8px;
}
</style>
