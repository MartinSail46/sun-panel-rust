<script setup lang="ts">
import GenericProgress from '../components/GenericProgress/index.vue'
import { correctionNumberByCardStyle } from './common'
import type { PanelPanelConfigStyleEnum } from '@/enums'
import { bytesToSize } from '@/utils/cmn'
import { useSystemMonitor } from '@/hooks/useSystemMonitor'
import { formatMonitorCapacity } from '@/utils/monitorReading'

interface Prop {
  cardTypeStyle: PanelPanelConfigStyleEnum
  refreshInterval: number
  textColor: string
  progressColor: string
  progressRailColor: string
  path: string
}

const props = defineProps<Prop>()
const { state: diskState, stale, retry } = useSystemMonitor<SystemMonitor.DiskInfo>(() => ({ type: 'disk', path: props.path }), () => props.refreshInterval)

</script>

<template>
  <GenericProgress
    :progress-color="progressColor"
    :progress-rail-color="progressRailColor"
    :progress-height="5"
    :loading="diskState.loading"
    :error="diskState.error"
    :updated-at="diskState.updatedAt"
    :stale="stale"
    :percentage="diskState.value ? correctionNumberByCardStyle(diskState.value.usedPercent, cardTypeStyle) : null"
    :card-type-style="cardTypeStyle"
    :info-card-right-text="formatMonitorCapacity(diskState.value, bytesToSize)"
    :capacity="true"
    :info-card-left-text="diskState.value?.mountpoint || path"
    :text-color="textColor"
    @retry="retry"
  />
</template>
