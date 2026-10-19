<script setup lang="ts">
import { computed } from 'vue'
import GenericProgress from '../components/GenericProgress/index.vue'
import { correctionNumber, correctionNumberByCardStyle } from './common'
import { useSystemMonitor } from '@/hooks/useSystemMonitor'
import type { PanelPanelConfigStyleEnum } from '@/enums'

interface Prop {
  cardTypeStyle: PanelPanelConfigStyleEnum
  refreshInterval: number
  textColor: string
  progressColor: string
  progressRailColor: string
}

const props = defineProps<Prop>()
const { state: cpuState, stale, retry } = useSystemMonitor<SystemMonitor.CPUInfo>(() => ({ type: 'cpu' }), () => props.refreshInterval)
const usage = computed(() => {
  const value = cpuState.value?.usages[0]
  return value !== undefined && Number.isFinite(value) ? value : null
})

</script>

<template>
  <GenericProgress
    :progress-color="progressColor"
    :progress-rail-color="progressRailColor"
    :progress-height="5"
    :loading="cpuState.loading"
    :error="cpuState.error"
    :updated-at="cpuState.updatedAt"
    :stale="stale"
    :percentage="usage !== null ? correctionNumberByCardStyle(usage, cardTypeStyle) : null"
    :card-type-style="cardTypeStyle"
    :info-card-right-text="usage !== null ? `${correctionNumber(usage)}%` : '—'"
    info-card-left-text="CPU"
    :text-color="textColor"
    style="width: 100%;"
    @retry="retry"
  />
</template>
