<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { cn } from '@/lib/utils'
defineOptions({ inheritAttrs: false })
const props = defineProps<{ modelValue?: string | number; options: { label: string; value: string | number; disabled?: boolean }[]; disabled?: boolean; placeholder?: string; class?: HTMLAttributes['class'] }>()
const emit = defineEmits<{ 'update:modelValue': [value: string | number] }>()
function update(value: unknown) {
  const selected = props.options.find(option => String(option.value) === String(value))
  if (selected) emit('update:modelValue', selected.value)
}
</script>
<template>
  <Select :model-value="modelValue === undefined ? undefined : String(modelValue)" :disabled="disabled" @update:model-value="update">
    <SelectTrigger v-bind="$attrs" :class="cn('w-full min-w-0', props.class)">
      <SelectValue :placeholder="placeholder" />
    </SelectTrigger>
    <SelectContent>
      <SelectItem v-for="option in options" :key="String(option.value)" :value="String(option.value)" :disabled="option.disabled">
        {{ option.label }}
      </SelectItem>
    </SelectContent>
  </Select>
</template>
