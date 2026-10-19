<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { AppLoader, RoundCardModal, SvgIcon } from '@/components/common'
import { Button } from '@/components/ui/button'

interface App {
  name: string
  componentName: string
  icon: string
}
const props = defineProps<{
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'update:visible', visible: boolean): void
}>()

const componentName = ref('Style')
const collapsed = ref(false)
const isSmallScreen = ref(false)
const appBusy = ref(false)
const defaultTitle = '系统应用'
const sidebarLabel = '展开或收起应用菜单'

const apps: App[] = [
  {
    name: '偏好设置',
    componentName: 'Preferences',
    icon: 'ri-settings-4-line',
  },
  {
    name: '风格设置',
    componentName: 'Style',
    icon: 'ion-color-palette-outline',
  },
  {
    name: '分组管理',
    componentName: 'ItemGroupManage',
    icon: 'material-symbols-ad-group-outline-rounded',
  },
  {
    name: '上传文件管理',
    componentName: 'UploadFileManager',
    icon: 'tabler:file-upload',
  },
  {
    name: '导出导入',
    componentName: 'ImportExport',
    icon: 'icon-park-outline-import-and-export',
  },
  {
    name: '回收站',
    componentName: 'RecycleBin',
    icon: 'lucide:trash-2',
  },
  {
    name: '关于',
    componentName: 'About',
    icon: 'lucide-info',
  },
]

const show = computed({
  get: () => props.visible,
  set: (visible: boolean) => {
    if (visible || !appBusy.value)
      emit('update:visible', visible)
  },
})

function handleClickApp(item: App) {
  if (appBusy.value)
    return
  componentName.value = item.componentName
  if (isSmallScreen.value)
    collapsed.value = true
}

function handleResize() {
  const smallScreen = window.innerWidth < 640
  if (smallScreen !== isSmallScreen.value) {
    isSmallScreen.value = smallScreen
    collapsed.value = smallScreen
  }
}

onMounted(() => {
  window.addEventListener('resize', handleResize)
  handleResize()
})

onUnmounted(() => {
  window.removeEventListener('resize', handleResize)
})
</script>

<template>
  <div>
    <RoundCardModal
      v-model:show="show"
      class="overflow-hidden sm:h-[min(88dvh,900px)] sm:max-w-[900px]"
      full-screen-on-mobile
      body-class="flex min-h-0 flex-1 overflow-hidden"
      :closable="!appBusy"
      :mask-closable="!appBusy"
      :close-on-esc="!appBusy"
    >
      <template #header>
        <Button type="button" variant="ghost" class="h-auto gap-1 p-0 text-base font-semibold hover:bg-transparent dark:hover:bg-transparent" :disabled="appBusy" :aria-label="sidebarLabel" :aria-expanded="!collapsed" @click="collapsed = !collapsed">
          <span class="text-3xl text-muted-foreground">
            <SvgIcon class=" transition-all duration-500" :icon="collapsed ? 'tabler-layout-sidebar-right-collapse-filled' : 'tabler-layout-sidebar-left-collapse-filled'" />
          </span>
          <span>
            {{ defaultTitle }}
          </span>
          <span v-if="isSmallScreen" class="ml-1 text-xs font-normal text-muted-foreground">{{ collapsed ? '切换设置' : '收起菜单' }}</span>
        </Button>
      </template>
      <div class="flex min-h-0 min-w-0 flex-1 gap-4">
        <nav v-if="!collapsed" class="min-h-0 shrink-0 overflow-y-auto overscroll-contain rounded-lg bg-muted/60 p-1.5" :class="isSmallScreen ? 'w-full' : 'w-52'" :aria-label="defaultTitle">
          <Button v-for="item in apps" :key="item.componentName" type="button" variant="ghost" class="app-menu-button mb-1 h-auto w-full justify-start px-3 py-2.5 text-left" :disabled="appBusy" :aria-current="componentName === item.componentName ? 'page' : undefined" @click="handleClickApp(item)">
            <SvgIcon class="text-lg" :icon="item.icon" /><span>{{ item.name }}</span>
          </Button>
        </nav>
        <div v-show="!isSmallScreen || collapsed" class="min-h-0 min-w-0 flex-1 rounded-lg" :class="componentName === 'Style' ? 'overflow-hidden' : 'overflow-y-auto overscroll-contain'">
          <AppLoader :component-name="componentName" class="h-full" @busy="appBusy = $event" />
        </div>
      </div>
    </RoundCardModal>
  </div>
</template>

<style scoped>
.app-menu-button[aria-current="page"] { background:var(--accent);color:var(--accent-foreground); }
</style>
