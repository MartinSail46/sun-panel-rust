<script setup lang="ts">
import { computed } from 'vue'
import type { GenericProgressStyleExtendParam } from '../../typings'
import GenericMonitorCard from '../../components/GenericMonitorCard/index.vue'
import GenericProgress from '../../components/GenericProgress/index.vue'
import { Label } from '@/components/ui/label'
import ColorPicker from '@/components/common/ColorPicker.vue'
import AppSelect from '@/components/common/AppSelect.vue'
import { PanelPanelConfigStyleEnum } from '@/enums'
import { defautSwatchesBackground } from '@/utils/defaultData'

const props = defineProps<{ genericProgressStyleExtendParam: GenericProgressStyleExtendParam; label?: string }>()
const emit = defineEmits<{ 'update:genericProgressStyleExtendParam': [value: GenericProgressStyleExtendParam] }>()
const data = computed(() => props.genericProgressStyleExtendParam)
const selectedStyle = computed(() => data.value.style === PanelPanelConfigStyleEnum.info ? PanelPanelConfigStyleEnum.info : PanelPanelConfigStyleEnum.icon)
const styleOptions = [
  { label: '详情', value: PanelPanelConfigStyleEnum.info },
  { label: '小图标', value: PanelPanelConfigStyleEnum.icon },
]
const fields = [
  { key: 'progressColor', label: '主色' },
  { key: 'progressRailColor', label: '副色' },
  { key: 'color', label: '文字颜色' },
  { key: 'backgroundColor', label: '背景颜色' },
] as const

function setStyle(value: string | number) {
  emit('update:genericProgressStyleExtendParam', { ...data.value, style: Number(value) === PanelPanelConfigStyleEnum.info ? PanelPanelConfigStyleEnum.info : PanelPanelConfigStyleEnum.icon })
}

function setColor(key: keyof GenericProgressStyleExtendParam, value: string) {
  emit('update:genericProgressStyleExtendParam', { ...data.value, [key]: value })
}
</script>

<template>
  <div class="space-y-5">
    <div class="space-y-2">
      <Label for="monitor-display-style">展示风格</Label>
      <AppSelect id="monitor-display-style" :model-value="selectedStyle" :options="styleOptions" @update:model-value="setStyle" />
    </div>
    <p class="text-xs text-muted-foreground">
      样式预览（示例值）
    </p>
    <div class="transparent-grid flex justify-center rounded-xl border p-3">
      <div :class="selectedStyle === PanelPanelConfigStyleEnum.info ? 'w-[200px]' : 'w-[80px]'">
        <GenericMonitorCard icon-text-icon-hide-title :card-type-style="selectedStyle" :background-color="data.backgroundColor" :icon-text-color="data.color">
          <template #info>
            <GenericProgress :progress-color="data.progressColor" :progress-rail-color="data.progressRailColor" :percentage="50" :progress-height="5" :info-card-left-text="label || 'CPU'" info-card-right-text="50%" :text-color="data.color" :card-type-style="PanelPanelConfigStyleEnum.info" />
          </template>
          <template #small>
            <GenericProgress :progress-color="data.progressColor" :progress-rail-color="data.progressRailColor" :percentage="50" :text-color="data.color" :info-card-left-text="label || 'CPU'" :card-type-style="PanelPanelConfigStyleEnum.icon" />
          </template>
        </GenericMonitorCard>
      </div>
    </div>
    <div v-for="field in fields" :key="field.key" class="space-y-2">
      <Label :for="`monitor-${field.key}`">{{ field.label }}</Label>
      <ColorPicker :id="`monitor-${field.key}`" :model-value="data[field.key]" :aria-label="field.label" :swatches="defautSwatchesBackground" @update:model-value="setColor(field.key, $event)" />
    </div>
  </div>
</template>

<style scoped>
.transparent-grid {
  background-image: linear-gradient(45deg, #fff 25%, transparent 25%, transparent 75%, #fff 75%),
    linear-gradient(45deg, #fff 25%, transparent 25%, transparent 75%, #fff 75%);
  background-size: 16px 16px;
  background-position: 0 0, 8px 8px;
  background-color: #e2e8f0;
}
</style>
