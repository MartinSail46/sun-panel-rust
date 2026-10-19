<script setup lang="ts">
import { watch } from 'vue'
import { rememberDialogOpener } from '@/hooks/useDialogFocus'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
const props = defineProps<{ open: boolean; x: number; y: number; options: { key: string | number; label: string; disabled?: boolean }[] }>()
const emit = defineEmits<{ 'update:open': [open: boolean]; select: [key: string | number] }>()
let previousFocus: HTMLElement | null = null
watch(() => props.open, (open) => {
  if (open) previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
})
function select(key: string | number) {
  rememberDialogOpener(previousFocus)
  emit('select', key)
}
function restoreFocus(event: Event) {
  event.preventDefault()
  if (!document.querySelector('[data-slot="dialog-content"][data-state="open"], [data-slot="alert-dialog-content"][data-state="open"]')) previousFocus?.focus()
}
</script>
<template>
  <DropdownMenu :open="open" @update:open="emit('update:open', $event)">
    <DropdownMenuTrigger as-child>
      <button type="button" tabindex="-1" aria-hidden="true" class="pointer-events-none fixed size-px opacity-0" :style="{ left: `${x}px`, top: `${y}px` }" />
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" :side-offset="0" class="min-w-40" @close-auto-focus="restoreFocus">
      <DropdownMenuItem v-for="option in options" :key="option.key" :disabled="option.disabled" @select="select(option.key)">
        {{ option.label }}
      </DropdownMenuItem>
    </DropdownMenuContent>
  </DropdownMenu>
</template>
