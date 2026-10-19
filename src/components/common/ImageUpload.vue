<script setup lang="ts">
import { ref } from 'vue'
import { Button } from '@/components/ui/button'
const props = withDefaults(defineProps<{ disabled?: boolean; accept?: string }>(), { accept: 'image/*' })
const emit = defineEmits<{ select: [file: File] }>()
const input = ref<HTMLInputElement>()
const dragging = ref(false)
function choose() { if (!props.disabled) input.value?.click() }
function selected(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  target.value = ''
  if (file && !props.disabled) emit('select', file)
}
function drop(event: DragEvent) {
  dragging.value = false
  const file = event.dataTransfer?.files?.[0]
  if (file && !props.disabled) emit('select', file)
}
</script>
<template>
  <div class="min-w-0" @dragover.prevent="dragging = !disabled" @dragleave.prevent="dragging = false" @drop.prevent="drop">
    <input ref="input" type="file" :accept="accept" :disabled="disabled" class="hidden" tabindex="-1" @change="selected">
    <Button type="button" variant="outline" :disabled="disabled" class="block h-auto w-full overflow-hidden p-0 text-left font-normal" :class="dragging ? 'ring-2 ring-ring' : ''" @click="choose">
      <slot><span class="block px-3 py-2">本地上传</span></slot>
    </Button>
  </div>
</template>
