import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useAppStore } from '@/store'
export function useTheme() {
  const app = useAppStore()
  const query = window.matchMedia('(prefers-color-scheme: dark)')
  const systemDark = ref(query.matches)
  const changed = (event: MediaQueryListEvent) => { systemDark.value = event.matches }
  query.addEventListener('change', changed)
  onBeforeUnmount(() => query.removeEventListener('change', changed))
  const isDark = computed(() => app.theme === 'auto' ? systemDark.value : app.theme === 'dark')
  watch(isDark, dark => document.documentElement.classList.toggle('dark', dark), { immediate: true })
  return { isDark }
}
