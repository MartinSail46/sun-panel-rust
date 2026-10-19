<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { toast } from '@/lib/feedback'
import { SvgIcon } from '@/components/common'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { useModuleConfig } from '@/store/modules'
import { searchUrl } from '@/store/modules/navigation/helpers'
import SvgSrcBaidu from '@/assets/search_engine_svg/baidu.svg'
import SvgSrcBing from '@/assets/search_engine_svg/bing.svg'
import SvgSrcGoogle from '@/assets/search_engine_svg/google.svg'

withDefaults(defineProps<{ background?: string, textColor?: string }>(), { background: '#2a2a2a6b', textColor: 'white' })
const emits = defineEmits<{
  (event: 'itemSearch', value: string): void
  (event: 'activate'): void
  (event: 'select', delta: number): void
}>()
interface State {
  currentSearchEngine: DeskModule.SearchBox.SearchEngine
}
const text = { apps: '应用', web: '网页', appsPlaceholder: '搜索应用 · Enter 打开 · ↑↓ 选择', webPlaceholder: '搜索网页（新标签打开）', clear: '清空搜索', engine: '选择搜索引擎', search: '搜索' }
const moduleConfig = useModuleConfig()
const ms = toast
const searchTerm = ref('')
const mode = ref<'apps' | 'web'>('apps')
const isFocused = ref(false)
const selectEngine = ref(false)
const engines: DeskModule.SearchBox.SearchEngine[] = [
  { iconSrc: SvgSrcGoogle, title: 'Google', url: 'https://www.google.com/search?q=%s' },
  { iconSrc: SvgSrcBaidu, title: 'Baidu', url: 'https://www.baidu.com/s?wd=%s' },
  { iconSrc: SvgSrcBing, title: 'Bing', url: 'https://www.bing.com/search?q=%s' },
]
const state = ref<State>({ currentSearchEngine: engines[0] })
const engineSaving = ref(false)
watch([searchTerm, mode], () => emits('itemSearch', mode.value === 'apps' ? searchTerm.value : ''))
watch(mode, () => selectEngine.value = false)

function submit() {
  if (!searchTerm.value.trim())
    return
  if (mode.value === 'apps') {
    emits('activate')
    return
  }
  const url = searchUrl(state.value.currentSearchEngine.url, searchTerm.value)
  window.open(url, '_blank', 'noopener,noreferrer')
}
function onKeydown(event: KeyboardEvent) {
  if (event.isComposing)
    return
  if (event.key === 'Escape') {
    event.preventDefault()
    searchTerm.value = ''
    selectEngine.value = false
  }
  if (event.key === 'Enter') {
    event.preventDefault()
    submit()
  }
  if (mode.value === 'apps' && ['ArrowDown', 'ArrowUp'].includes(event.key)) {
    event.preventDefault()
    emits('select', event.key === 'ArrowDown' ? 1 : -1)
  }
}
async function updateEngine(engine: DeskModule.SearchBox.SearchEngine) {
  if (engineSaving.value)
    return
  engineSaving.value = true
  const previous = state.value.currentSearchEngine
  state.value.currentSearchEngine = engine
  try {
    const response = await moduleConfig.saveToCloud('deskModuleSearchBox', { ...state.value })
    if (response.code !== 0)
      throw new Error(response.msg || '保存失败')
    selectEngine.value = false
  }
  catch (error) {
    state.value.currentSearchEngine = previous
    ms.error(error instanceof Error ? error.message : '保存失败')
  }
  finally {
    engineSaving.value = false
  }
}
onMounted(async () => {
  try {
    const response = await moduleConfig.getValueByNameFromCloud<State>('deskModuleSearchBox')
    if (response.code !== 0)
      throw new Error(response.msg || '加载失败，请重试')
    if (response.data?.currentSearchEngine?.url) {
      const saved = response.data.currentSearchEngine
      const engine = engines.find(value => value.url === saved.url)
      state.value.currentSearchEngine = engine || engines[0]
    }
  }
  catch {
    ms.error('加载失败，请重试')
  }
})
</script>

<template>
  <div class="search-box w-full">
    <div class="search-container rounded-2xl flex items-center gap-1 w-full" :style="{ background, color: textColor }" :class="{ focused: isFocused }">
      <div class="flex shrink-0 gap-1" role="group" :aria-label="text.search">
        <Button type="button" variant="ghost" size="sm" class="mode-button h-8 px-2 text-current hover:bg-white/15 hover:text-current dark:hover:bg-white/15" :class="{ active: mode === 'apps' }" :aria-pressed="mode === 'apps'" @click="mode = 'apps'">
          {{ text.apps }}
        </Button>
        <Button type="button" variant="ghost" size="sm" class="mode-button h-8 px-2 text-current hover:bg-white/15 hover:text-current dark:hover:bg-white/15" :class="{ active: mode === 'web' }" :aria-pressed="mode === 'web'" @click="mode = 'web'">
          {{ text.web }}
        </Button>
      </div>
      <Popover v-if="mode === 'web'" v-model:open="selectEngine">
        <PopoverTrigger as-child>
          <Button type="button" variant="ghost" size="icon-sm" class="shrink-0 text-current hover:bg-white/15 hover:text-current dark:hover:bg-white/15" :title="text.engine" :aria-label="text.engine">
            <img :src="state.currentSearchEngine.iconSrc" alt="" class="size-5">
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" class="flex w-auto gap-2 p-2" :aria-label="text.engine">
          <Button v-for="engine in engines" :key="engine.title" type="button" variant="outline" size="icon" :title="engine.title" :aria-label="engine.title" :disabled="engineSaving" class="size-10 bg-white hover:bg-white/80 dark:bg-white dark:hover:bg-white/80" @click="updateEngine(engine)">
            <img :src="engine.iconSrc" alt="" class="size-5">
          </Button>
        </PopoverContent>
      </Popover>
      <Input v-model="searchTerm" type="search" class="search-input h-10.5 border-0 bg-transparent px-1 text-current shadow-none placeholder:text-current/55 focus-visible:ring-0 dark:bg-transparent" :aria-label="mode === 'apps' ? text.apps : text.web" :placeholder="mode === 'apps' ? text.appsPlaceholder : text.webPlaceholder" @focus="isFocused = true" @blur="isFocused = false" @keydown="onKeydown" />
      <Button v-if="searchTerm" type="button" variant="ghost" size="icon-sm" class="shrink-0 text-current hover:bg-white/15 hover:text-current dark:hover:bg-white/15" :aria-label="text.clear" @click="searchTerm = ''">
        <SvgIcon icon="line-md:close-small" />
      </Button>
      <Button type="button" variant="ghost" size="icon-sm" class="shrink-0 text-current hover:bg-white/15 hover:text-current dark:hover:bg-white/15" :aria-label="text.search" @click="submit">
        <SvgIcon icon="iconamoon:search-fill" />
      </Button>
    </div>
  </div>
</template>

<style scoped>
.search-container { border:1px solid #ffffff66;padding:2px 8px;backdrop-filter:blur(2px);transition:box-shadow .3s; }
.focused { box-shadow:0 0 25px -5px #0007; }
.search-input::-webkit-search-cancel-button { display:none; }
.mode-button { font-size:13px;opacity:.7; }
.mode-button.active { background:#ffffff26;opacity:1; }
</style>
