<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import type { DiskExtendParam, GenericProgressStyleExtendParam } from '../../typings'
import GenericProgressStyleEditor from '../GenericProgressStyleEditor/index.vue'
import AppSelect from '@/components/common/AppSelect.vue'
import { Label } from '@/components/ui/label'
import { Button } from '@/components/ui/button'
import { getDiskMountpoints } from '@/api/system/systemMonitor'
import { isRequestCancelled } from '@/utils/requestLifetime'

const props = defineProps<{ diskExtendParam: DiskExtendParam }>()
const emit = defineEmits<{ 'update:diskExtendParam': [value: DiskExtendParam] }>()
const mountPointList = ref<{ label: string, value: string }[]>([])
const loading = ref(false)
const loadError = ref('')
const validationError = ref('')
let controller: AbortController | undefined
const extendParam = computed({
  get: () => props.diskExtendParam,
  set: value => emit('update:diskExtendParam', value),
})
const path = computed({
  get: () => extendParam.value.path,
  set: (value: string | number) => {
    extendParam.value = { ...extendParam.value, path: String(value) }
    validationError.value = ''
  },
})
const options = computed(() => extendParam.value.path && !mountPointList.value.some(item => item.value === extendParam.value.path)
  ? [{ label: extendParam.value.path, value: extendParam.value.path }, ...mountPointList.value]
  : mountPointList.value)

async function getMountPointList() {
  controller?.abort()
  const request = new AbortController()
  controller = request
  loading.value = true
  loadError.value = ''
  try {
    const { code, data, msg } = await getDiskMountpoints<SystemMonitor.Mountpoint[]>({ signal: request.signal, silent: true })
    if (request.signal.aborted)
      return
    if (code !== 0)
      throw new Error(msg || '操作失败')
    mountPointList.value = data.map(item => ({ label: item.mountpoint, value: item.mountpoint }))
    if (!extendParam.value.path && data.length)
      path.value = data[0].mountpoint
  }
  catch (error) {
    if (!isRequestCancelled(error))
      loadError.value = error instanceof Error ? error.message : '操作失败'
  }
  finally {
    if (controller === request) {
      loading.value = false
      controller = undefined
    }
  }
}

function setStyle(value: GenericProgressStyleExtendParam) {
  extendParam.value = { ...extendParam.value, ...value }
}

onMounted(() => { void getMountPointList() })
onBeforeUnmount(() => { controller?.abort() })
defineExpose({
  verification(): boolean {
    validationError.value = extendParam.value.path.trim() ? '' : '必填项'
    return !validationError.value
  },
})
</script>

<template>
  <div class="space-y-5">
    <div class="space-y-2">
      <Label for="monitor-disk-path">挂载点</Label>
      <AppSelect id="monitor-disk-path" v-model="path" :options="options" :disabled="loading" aria-label="挂载点" :aria-invalid="!!validationError" />
      <p v-if="loading" class="text-sm text-muted-foreground" role="status">
        加载中
      </p>
      <p v-if="validationError" class="text-sm text-destructive" role="alert">
        {{ validationError }}
      </p>
      <div v-if="loadError" class="flex items-center gap-2 text-sm text-destructive" role="alert">
        <span>{{ loadError }}</span>
        <Button type="button" variant="outline" size="sm" @click="getMountPointList">
          重试
        </Button>
      </div>
    </div>
    <GenericProgressStyleEditor :label="extendParam.path || '磁盘状态'" :generic-progress-style-extend-param="extendParam" @update:generic-progress-style-extend-param="setStyle" />
  </div>
</template>
