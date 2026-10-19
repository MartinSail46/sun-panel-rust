<script setup lang="ts">
import { computed } from 'vue'
import { Button } from '@/components/ui/button'
import { PanelPanelConfigStyleEnum } from '@/enums'

interface Prop {
  textColor: string
  progressColor: string
  progressRailColor: string
  progressHeight?: number
  percentage: number | null
  cardTypeStyle: PanelPanelConfigStyleEnum
  infoCardLeftText?: string
  infoCardRightText?: string
  loading?: boolean
  error?: string | null
  updatedAt?: number | null
  stale?: boolean
  capacity?: boolean
}

const props = defineProps<Prop>()
const emit = defineEmits<{ retry: [] }>()
const value = computed(() => props.percentage !== null && Number.isFinite(props.percentage)
  ? Math.min(100, Math.max(0, props.percentage))
  : null)
const status = computed(() => props.error
  ? props.updatedAt ? '数据已过期' : '暂不可用'
  : props.loading && props.updatedAt == null
    ? '加载中'
    : props.stale ? '数据已过期' : '')
const hint = computed(() => [
  '指标来自运行环境可见的操作系统和挂载文件系统，可能属于虚拟机；未应用容器资源配额。',
  props.capacity ? '已用 / 总量' : '',
  props.updatedAt ? `最后采样：${new Date(props.updatedAt).toLocaleTimeString('zh-CN')}` : '',
  props.error || '',
].filter(Boolean).join('\n'))
</script>

<template>
  <div class="w-full" :title="hint" :style="{ color: textColor }" :aria-busy="loading">
    <template v-if="cardTypeStyle === PanelPanelConfigStyleEnum.info">
      <div class="mb-1 flex justify-between gap-2 text-xs">
        <span class="truncate">{{ infoCardLeftText }}</span>
        <span class="shrink-0">{{ infoCardRightText }}</span>
      </div>
      <div
        class="w-full max-w-[135px] overflow-hidden rounded-full"
        :style="{ backgroundColor: progressRailColor, height: `${progressHeight || 5}px` }"
        role="progressbar"
        :aria-label="infoCardLeftText || infoCardRightText"
        :aria-valuenow="value ?? undefined"
        :aria-valuemin="0"
        :aria-valuemax="100"
        :aria-valuetext="value === null ? status : infoCardRightText"
      >
        <div v-if="value !== null" class="h-full rounded-full transition-[width] duration-300 motion-reduce:transition-none" :style="{ backgroundColor: progressColor, width: `${value}%` }" />
      </div>
      <div v-if="capacity && !status" class="mt-1 text-[9px] opacity-80">
        已用 / 总量
      </div>
    </template>
    <div v-else class="mt-3 flex justify-center">
      <svg
        v-if="value !== null"
        viewBox="0 0 100 100"
        class="size-[50px] overflow-visible"
        role="progressbar"
        :aria-label="infoCardLeftText || infoCardRightText"
        :aria-valuenow="value"
        :aria-valuemin="0"
        :aria-valuemax="100"
        :aria-valuetext="infoCardRightText"
      >
        <circle cx="50" cy="50" r="40" fill="none" :stroke="progressRailColor" stroke-width="15" stroke-linecap="round" pathLength="100" stroke-dasharray="75 100" transform="rotate(135 50 50)" />
        <circle v-if="value > 0" cx="50" cy="50" r="40" fill="none" :stroke="progressColor" stroke-width="15" stroke-linecap="round" pathLength="100" :stroke-dasharray="`${value * 0.75} 100`" transform="rotate(135 50 50)" />
        <text x="50" y="55" text-anchor="middle" dominant-baseline="middle" fill="currentColor" font-size="16" aria-hidden="true">{{ value }}%</text>
      </svg>
      <div v-else class="flex h-[50px] items-center text-lg" :aria-label="status">
        —
      </div>
    </div>
    <div v-if="status" class="mt-1 flex flex-wrap items-center gap-1 text-[10px]" role="status" aria-live="polite">
      <span>{{ status }}</span>
      <Button v-if="error || stale" type="button" variant="link" class="h-auto p-0 text-[10px] text-current underline" :disabled="loading" @click.stop="emit('retry')">
        重试
      </Button>
    </div>
  </div>
</template>
