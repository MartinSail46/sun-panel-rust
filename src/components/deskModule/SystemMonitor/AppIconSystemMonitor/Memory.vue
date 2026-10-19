<script setup lang="ts">
import GenericProgress from '../components/GenericProgress/index.vue'
import { correctionNumberByCardStyle } from './common'
import { useSystemMonitor } from '@/hooks/useSystemMonitor'
import { formatMonitorCapacity } from '@/utils/monitorReading'
import type { PanelPanelConfigStyleEnum } from '@/enums'
import { bytesToSize } from '@/utils/cmn'

interface Prop {
  cardTypeStyle: PanelPanelConfigStyleEnum
  refreshInterval: number
  textColor: string
  progressColor: string
  progressRailColor: string
}

const props = defineProps<Prop>()
const { state: memoryState, stale, retry } = useSystemMonitor<SystemMonitor.MemoryInfo>(() => ({ type: 'memory' }), () => props.refreshInterval)

</script>

<template>
  <GenericProgress
    :progress-color="progressColor"
    :progress-rail-color="progressRailColor"
    :progress-height="5"
    :loading="memoryState.loading"
    :error="memoryState.error"
    :updated-at="memoryState.updatedAt"
    :stale="stale"
    :percentage="memoryState.value ? correctionNumberByCardStyle(memoryState.value.usedPercent, cardTypeStyle) : null"
    :card-type-style="cardTypeStyle"
    :info-card-right-text="formatMonitorCapacity(memoryState.value, bytesToSize)"
    :capacity="true"
    info-card-left-text="RAM"
    :text-color="textColor"
    @retry="retry"
  />
</template>
