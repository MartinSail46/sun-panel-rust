<script setup lang="ts">
import { Ellipsis } from '@lucide/vue'
import { Button } from '@/components/ui/button'

defineProps<{ label: string; disabled?: boolean }>()
const emit = defineEmits<{ click: [event: MouseEvent] }>()
</script>

<template>
  <Button
    type="button"
    variant="ghost"
    size="icon"
    class="absolute -top-2 -right-2 z-10 size-7 rounded-full bg-black/70 text-white shadow-sm hover:bg-black/85 hover:text-white dark:hover:bg-black/85"
    :aria-label="label"
    :disabled="disabled"
    @click.stop="emit('click', $event)"
  >
    <Ellipsis class="size-4" />
  </Button>
</template>
