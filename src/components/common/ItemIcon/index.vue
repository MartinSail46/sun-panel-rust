<script setup lang="ts">
import { computed } from 'vue'
import SvgIconOnline from '../SvgIconOnline/index.vue'
const props = withDefaults(defineProps<{ itemIcon?: Panel.ItemIcon | null; size?: number; forceBackground?: string }>(), { size: 70 })
const dimensions = computed(() => ({ width: `${props.size}px`, height: `${props.size}px`, backgroundColor: (props.forceBackground ?? props.itemIcon?.backgroundColor) || '#2a2a2a6b' }))
</script>
<template>
  <div class="item-icon flex shrink-0 items-center justify-center overflow-hidden text-white" :style="dimensions">
    <slot>
      <span v-if="itemIcon?.itemType === 1" class="px-1 text-center">{{ itemIcon.text }}</span>
      <img v-else-if="itemIcon?.itemType === 2 && itemIcon.src" :src="itemIcon.src" alt="" draggable="false" loading="lazy" decoding="async" class="h-full w-full object-contain">
      <SvgIconOnline v-else-if="itemIcon?.itemType === 3" class="text-[35px]" :icon="itemIcon.text" />
    </slot>
  </div>
</template>
