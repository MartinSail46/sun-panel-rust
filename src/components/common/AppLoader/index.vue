<script setup lang="ts">
import type { Component } from 'vue'
import { computed, defineAsyncComponent } from 'vue'
import { Spinner } from '@/components/ui/spinner'

const props = defineProps<{
  componentName: string | null
}>()
const emit = defineEmits<{ busy: [value: boolean] }>()
const apps = new Map<string, Component>([
  ['Preferences', defineAsyncComponent({ loader: () => import('../../apps/Preferences/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['Style', defineAsyncComponent({ loader: () => import('../../apps/Style/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['ItemGroupManage', defineAsyncComponent({ loader: () => import('../../apps/ItemGroupManage/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['UploadFileManager', defineAsyncComponent({ loader: () => import('../../apps/UploadFileManager/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['ImportExport', defineAsyncComponent({ loader: () => import('../../apps/ImportExport/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['RecycleBin', defineAsyncComponent({ loader: () => import('../../apps/RecycleBin/index.vue'), loadingComponent: Spinner, delay: 500 })],
  ['About', defineAsyncComponent({ loader: () => import('../../apps/About/index.vue'), loadingComponent: Spinner, delay: 500 })],
])
const dynamicComponent = computed(() => apps.get(props.componentName || ''))
</script>

<template>
  <div class="h-full">
    <component :is="dynamicComponent" v-if="dynamicComponent" @busy="emit('busy', $event)" />
    <div v-else>
      未找到此应用
    </div>
  </div>
</template>
