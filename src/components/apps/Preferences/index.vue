<script setup lang="ts">
import { Card, CardContent } from '@/components/ui/card'
import AppSelect from '@/components/common/AppSelect.vue'
import { useAppStore } from '@/store'
import type { Theme } from '@/store/modules/app/helper'

const appStore = useAppStore()
const themeOptions: { label: string; value: Theme }[] = [
  { label: '深色', value: 'dark' },
  { label: '浅色', value: 'light' },
  { label: '跟随系统', value: 'auto' },
]

function handleChangeTheme(value: string | number) {
  if (value !== 'dark' && value !== 'light' && value !== 'auto') return
  appStore.setTheme(value)
}
</script>

<template>
  <div class="p-3 sm:p-4">
    <Card class="py-4">
      <CardContent class="grid gap-5 px-4">
        <div class="grid gap-2">
          <label for="preference-theme" class="text-sm font-medium">主题</label>
          <AppSelect id="preference-theme" :model-value="appStore.theme" class="max-w-xs" :options="themeOptions" aria-label="主题" @update:model-value="handleChangeTheme" />
        </div>
      </CardContent>
    </Card>
  </div>
</template>
