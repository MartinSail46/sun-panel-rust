<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { createForegroundScheduler, nextClockBoundary } from '@/utils/foregroundScheduler'

const props = defineProps<{
  hideSecond?: boolean
}>()

const weekdays = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六']
const timestamp = ref(Date.now())
const currentDate = computed(() => {
  const now = new Date(timestamp.value)
  const hours = String(now.getHours()).padStart(2, '0')
  const minutes = String(now.getMinutes()).padStart(2, '0')
  const seconds = String(now.getSeconds()).padStart(2, '0')
  return {
    time: props.hideSecond ? `${hours}:${minutes}` : `${hours}:${minutes}:${seconds}`,
    date: `${now.getMonth() + 1}-${now.getDate()}`,
    week: weekdays[now.getDay()],
  }
})

const scheduler = createForegroundScheduler({
  run: () => { timestamp.value = Date.now() },
  interval: () => props.hideSecond ? 60000 : 1000,
  successDelay: now => nextClockBoundary(now, !!props.hideSecond),
  isVisible: () => document.visibilityState !== 'hidden',
})

function handleVisibilityChange() {
  scheduler.visibilityChanged()
}

watch(() => props.hideSecond, () => scheduler.refresh())
onMounted(() => {
  document.addEventListener('visibilitychange', handleVisibilityChange)
  scheduler.start()
})
onBeforeUnmount(() => {
  scheduler.stop()
  document.removeEventListener('visibilitychange', handleVisibilityChange)
})
</script>

<template>
  <div class="clock w-full text-center tabular-nums">
    <span class="clock-time whitespace-nowrap text-2xl sm:text-2xl md:text-3xl font-[600]">
      {{ currentDate.time }}
    </span>
    <div class="hidden md:grid">
      <!-- Reserve the widest date and weekday so midnight cannot move the header. -->
      <div v-for="weekday in weekdays" :key="weekday" class="invisible col-start-1 row-start-1 whitespace-nowrap" aria-hidden="true">
        <span class="mr-1">00-00</span>
        <span>{{ weekday }}</span>
      </div>
      <div class="col-start-1 row-start-1 whitespace-nowrap">
        <span class="clock-date mr-1">
          {{ currentDate.date }}
        </span>
        <span class="clock-week">
          {{ currentDate.week }}
        </span>
      </div>
    </div>
  </div>
</template>
