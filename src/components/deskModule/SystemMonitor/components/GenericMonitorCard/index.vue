<script setup lang="ts">
import { ItemCard } from '@/components/common'
import type { PanelPanelConfigStyleEnum } from '@/enums'

interface Prop {
  iconTextColor?: string
  iconTextIconHideTitle?: boolean
  iconText?: string
  cardTypeStyle: PanelPanelConfigStyleEnum
  class?: string
  backgroundColor?: string
}

const props = withDefaults(defineProps<Prop>(), {})
</script>

<template>
  <div class="generic-monitor-card w-full">
    <ItemCard
      :card-type-style="cardTypeStyle"
      :icon-text="iconText"
      :icon-text-color="iconTextColor"
      :class="props.class"
      :background-color="backgroundColor"
      :icon-text-icon-hide-title="iconTextIconHideTitle"
    >
      <template #info>
        <!-- 图标 -->
        <div class="w-[60px] h-[70px]">
          <div class="w-[60px] h-full flex items-center justify-center text-white">
            <slot name="icon" />
          </div>
        </div>

        <div
          class=" w-full text-white flex items-center"
          :style=" { maxWidth: 'calc(100% - 80px)' }"
        >
          <slot name="info" />
        </div>
      </template>
      <template #small>
        <slot name="small" />
      </template>
    </ItemCard>
  </div>
</template>
