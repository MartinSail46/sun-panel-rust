<script setup lang="ts">
import { computed, defineEmits, defineProps, ref, watch } from 'vue'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import RoundCardModal from '@/components/common/RoundCardModal/index.vue'
import { toast } from '@/lib/feedback'
import { MonitorType } from '../typings'
import type { DiskExtendParam, GenericProgressStyleExtendParam, MonitorData } from '../typings'
import { add, saveByIndex } from '../common'

import GenericProgressStyleEditor from './GenericProgressStyleEditor/index.vue'
import DiskEditor from './DiskEditor/index.vue'

interface Props {
  visible: boolean
  monitorData: MonitorData | null
  index: number | null
}

const props = defineProps<Props>()
const emit = defineEmits<Emit>()
const DiskEditorRef = ref<InstanceType<typeof DiskEditor>>()

// 默认通用的进度扩展参数
const defaultGenericProgressStyleExtendParam: GenericProgressStyleExtendParam = {
  progressColor: '#fff',
  progressRailColor: '#CFCFCFA8',
  color: '#fff',
  backgroundColor: '#2a2a2a6b',
}

const defaultDiskExtendParam: DiskExtendParam = {
  progressColor: '#fff',
  progressRailColor: '#CFCFCFA8',
  color: '#fff',
  backgroundColor: '#2a2a2a6b',
  path: '',
}

const defaultMonitorData: MonitorData = {
  extendParam: defaultGenericProgressStyleExtendParam,
  monitorType: MonitorType.cpu,
}

const active = ref<string>(MonitorType.cpu)
const currentMonitorData = ref<MonitorData>({ ...defaultMonitorData })
const currentGenericProgressStyleExtendParam = ref<GenericProgressStyleExtendParam>({ ...defaultGenericProgressStyleExtendParam })
const currentDiskExtendParam = ref<DiskExtendParam>({ ...defaultDiskExtendParam })

const submitLoading = ref(false)

interface Emit {
  (e: 'update:visible', visible: boolean): void
  (e: 'done', item: boolean): void
}

// 更新值父组件传来的值
const show = computed({
  get: () => props.visible,
  set: (visible: boolean) => {
    emit('update:visible', visible)
  },
})

watch(() => props.visible, (value) => {
  if (!value) {
    handleResetExtendParam()
    return
  }

  currentMonitorData.value = { ...(props.monitorData || defaultMonitorData) }
  active.value = props.monitorData?.monitorType || MonitorType.cpu
  if (props.monitorData?.monitorType === MonitorType.cpu || props.monitorData?.monitorType === MonitorType.memory)
    currentGenericProgressStyleExtendParam.value = { ...defaultGenericProgressStyleExtendParam, ...props.monitorData?.extendParam }
  else if (props.monitorData?.monitorType === MonitorType.disk)
    currentDiskExtendParam.value = { ...defaultDiskExtendParam, ...props.monitorData?.extendParam }
}, { immediate: true })

function handleResetExtendParam() {
  currentGenericProgressStyleExtendParam.value = { ...defaultGenericProgressStyleExtendParam }
  currentDiskExtendParam.value = { ...defaultDiskExtendParam }
}

// 保存提交
async function handleSubmit() {
  let verificationRes = true
  currentMonitorData.value.monitorType = active.value as MonitorType
  if (currentMonitorData.value.monitorType === MonitorType.cpu || currentMonitorData.value.monitorType === MonitorType.memory) {
    currentMonitorData.value.extendParam = { ...currentGenericProgressStyleExtendParam.value }
  }
  else if (currentMonitorData.value.monitorType === MonitorType.disk) {
    currentMonitorData.value.extendParam = { ...currentDiskExtendParam.value }
    verificationRes = DiskEditorRef.value?.verification() ?? false
  }

  if (!verificationRes)
    return

  if (submitLoading.value)
    return
  submitLoading.value = true
  try {
    if (props.index !== null) {
      const res = await saveByIndex(props.index, currentMonitorData.value)
      if (res) {
        show.value = false
        emit('done', true)
      }
      else {
        toast.error('保存失败')
      }
    }
    else {
      const res = await add(currentMonitorData.value)
      if (res) {
        show.value = false
        emit('done', true)
      }
      else {
        toast.error('保存失败')
      }
    }
  }
  catch {
    toast.error('保存失败')
  }
  finally {
    submitLoading.value = false
  }
}
</script>

<template>
  <RoundCardModal v-model:show="show" class="sm:max-w-[600px]" :closable="!submitLoading" :mask-closable="!submitLoading" :close-on-esc="!submitLoading" :title="monitorData ? '编辑' : '添加'">
    <form id="monitor-editor-form" @submit.prevent="handleSubmit">
      <Tabs v-model="active" class="space-y-5">
        <TabsList class="grid w-full grid-cols-3">
          <TabsTrigger :value="MonitorType.cpu" :disabled="submitLoading">
            CPU状态
          </TabsTrigger>
          <TabsTrigger :value="MonitorType.memory" :disabled="submitLoading">
            内存状态
          </TabsTrigger>
          <TabsTrigger :value="MonitorType.disk" :disabled="submitLoading">
            磁盘状态
          </TabsTrigger>
        </TabsList>
        <TabsContent :value="MonitorType.cpu">
          <GenericProgressStyleEditor v-model:generic-progress-style-extend-param="currentGenericProgressStyleExtendParam" />
        </TabsContent>
        <TabsContent :value="MonitorType.memory">
          <GenericProgressStyleEditor v-model:generic-progress-style-extend-param="currentGenericProgressStyleExtendParam" label="RAM" />
        </TabsContent>
        <TabsContent :value="MonitorType.disk">
          <DiskEditor ref="DiskEditorRef" v-model:disk-extend-param="currentDiskExtendParam" />
        </TabsContent>
      </Tabs>
    </form>
    <template #footer>
      <div class="flex w-full items-center justify-between gap-3">
        <Button type="button" variant="outline" :disabled="submitLoading" @click="handleResetExtendParam">
          重置
        </Button>
        <div class="flex gap-2">
          <Button type="button" variant="outline" :disabled="submitLoading" @click="show = false">
            取消
          </Button>
          <Button type="submit" form="monitor-editor-form" :disabled="submitLoading" :aria-busy="submitLoading">
            确定
          </Button>
        </div>
      </div>
    </template>
  </RoundCardModal>
</template>
