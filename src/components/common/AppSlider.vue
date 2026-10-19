<script setup lang="ts">
import { computed } from 'vue'
import { Slider } from '@/components/ui/slider'
defineOptions({ inheritAttrs: false })
const props = withDefaults(defineProps<{ min?: number; max?: number; step?: number; disabled?: boolean }>(), { min: 0, max: 100, step: 1 })
const model = defineModel<number>()
const value = computed(() => [model.value ?? props.min])
function update(next: number[] | undefined) { if (next?.[0] !== undefined) model.value = next[0] }
</script>
<template>
  <div class="flex min-h-6 min-w-0 items-center gap-3">
    <Slider v-bind="$attrs" :model-value="value" :min="min" :max="max" :step="step" :disabled="disabled" class="min-w-16 flex-1" @update:model-value="update" />
    <output class="w-8 shrink-0 text-right text-xs tabular-nums text-muted-foreground">{{ model ?? min }}</output>
  </div>
</template>
