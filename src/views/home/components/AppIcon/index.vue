<script setup lang="ts">
import { computed } from 'vue'
import { ItemIcon } from '@/components/common'
import { PanelPanelConfigStyleEnum } from '@/enums'

interface Prop {
  itemInfo?: Panel.ItemInfo
}

const props = defineProps<Prop>()

const defaultBackground = '#2a2a2a6b'
const displayStyle = computed(() => props.itemInfo?.icon?.style ?? PanelPanelConfigStyleEnum.icon)
const iconTextColor = computed(() => props.itemInfo?.icon?.textColor || '#ffffff')

const calculateLuminance = (color: string) => {
  const hex = color.replace(/^#/, '')
  const r = parseInt(hex.substring(0, 2), 16)
  const g = parseInt(hex.substring(2, 4), 16)
  const b = parseInt(hex.substring(4, 6), 16)
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255
}

const textColor = computed(() => {
  const luminance = calculateLuminance(props.itemInfo?.icon?.backgroundColor || defaultBackground)
  return luminance > 0.5 ? 'black' : 'white'
})
</script>

<template>
  <div class="app-icon w-full">
    <!-- 详情图标 -->
    <div
      v-if="displayStyle === PanelPanelConfigStyleEnum.info"
      class="app-icon-info w-full rounded-2xl  transition-all duration-200 hover:shadow-[0_0_20px_10px_rgba(0,0,0,0.2)] flex"
      :style="{ background: itemInfo?.icon?.backgroundColor || defaultBackground }"
    >
      <!-- 图标 -->
      <div class="app-icon-info-icon w-[70px] h-[70px]">
        <div class="w-[70px] h-full flex items-center justify-center ">
          <ItemIcon :item-icon="itemInfo?.icon" force-background="transparent" :size="50" class="overflow-hidden rounded-xl" />
        </div>
      </div>

      <!-- 文字 -->
      <!-- 如果为纯白色，将自动根据背景的明暗计算字体的黑白色 -->
      <div class="text-white flex items-center" :style="{ color: (iconTextColor === '#ffffff') ? textColor : iconTextColor, maxWidth: 'calc(100% - 80px)' }">
        <div class="app-icon-info-text-box w-full">
          <div class="app-icon-info-text-box-title font-semibold w-full">
            <span class="block truncate">
              {{ itemInfo?.title }}
            </span>
          </div>
          <div v-if="!itemInfo?.icon?.hideDescription" class="app-icon-info-text-box-description">
            <span class="line-clamp-2 text-xs">
              {{ itemInfo?.description }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- 极简(小)图标（APP） -->
    <div v-if="displayStyle === PanelPanelConfigStyleEnum.icon" class="app-icon-small">
      <div
        class="app-icon-small-icon overflow-hidden rounded-2xl w-[70px] h-[70px] mx-auto rounded-2xl transition-all duration-200 hover:shadow-[0_0_20px_10px_rgba(0,0,0,0.2)]"
        :title="itemInfo?.description"
      >
        <ItemIcon :item-icon="itemInfo?.icon" />
      </div>
      <div
        v-if="!itemInfo?.icon?.hideTitle"
        class="app-icon-small-title text-center app-icon-text-shadow cursor-pointer mt-[2px]"
        :style="{ color: iconTextColor }"
      >
        <span>{{ itemInfo?.title }}</span>
      </div>
    </div>
  </div>
</template>
