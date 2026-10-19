<script setup lang="ts">
import { computed, ref, useAttrs, useId, watch } from 'vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Slider } from '@/components/ui/slider'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { parseColor, serializeColor } from '@/lib/color'
import type { RgbaColor } from '@/lib/color'

defineOptions({ inheritAttrs: false })
const props = withDefaults(defineProps<{
  modelValue?: string
  swatches?: string[]
  disabled?: boolean
}>(), { disabled: false })
const emit = defineEmits<{ (e: 'update:modelValue', value: string): void }>()
const attrs = useAttrs()
const id = useId()
const open = ref(false)
const selected = ref<RgbaColor>(parseColor(props.modelValue) || { r: 0, g: 0, b: 0, a: 0 })
const draft = ref(props.modelValue || '')
const invalid = ref(false)
const copy = { color: '颜色', opacity: '不透明度', format: 'HEX / RGB / RGBA', invalid: '请输入有效的 HEX、RGB 或 RGBA 颜色值', presets: '预设颜色' }
const label = computed(() => typeof attrs['aria-label'] === 'string' ? attrs['aria-label'] : copy.color)
const swatches = computed(() => [...new Set(props.swatches || [])].filter(value => parseColor(value)))
const opaqueHex = computed(() => serializeColor(selected.value, false))
const selectedHex = computed(() => serializeColor(selected.value))
const opacity = computed(() => Math.round(selected.value.a / 255 * 100))

watch(() => props.modelValue, (value) => {
  draft.value = value || ''
  invalid.value = false
  const parsed = parseColor(value)
  if (parsed)
    selected.value = parsed
  else if (!value)
    selected.value = { r: 0, g: 0, b: 0, a: 0 }
})
watch(() => props.disabled, (disabled) => {
  if (disabled)
    open.value = false
})
function commit(color: RgbaColor) {
  if (props.disabled)
    return
  selected.value = color
  draft.value = serializeColor(color)
  invalid.value = false
  emit('update:modelValue', draft.value)
}
function commitDraft() {
  const parsed = parseColor(draft.value)
  if (!parsed) {
    invalid.value = true
    return
  }
  commit(parsed)
}
function confirmColor() {
  commitDraft()
  if (!invalid.value) open.value = false
}
function updateColor(event: Event) {
  const parsed = parseColor((event.target as HTMLInputElement).value)
  if (parsed)
    commit({ ...parsed, a: selected.value.a })
}
function updateAlpha(value: number[] | undefined) {
  if (value?.[0] !== undefined)
    commit({ ...selected.value, a: Math.round(value[0] / 100 * 255) })
}
function chooseSwatch(value: string) {
  const color = parseColor(value)
  if (color)
    commit(color)
}
</script>

<template>
  <Popover v-model:open="open">
    <PopoverTrigger as-child>
      <Button type="button" variant="outline" class="min-w-0 justify-start gap-2 font-normal" :disabled="disabled" v-bind="attrs" :aria-label="label">
        <span class="color-checker size-5 shrink-0 overflow-hidden rounded border border-border" aria-hidden="true">
          <span class="block size-full" :style="{ backgroundColor: selectedHex }" />
        </span>
        <span class="truncate font-mono text-xs">{{ selectedHex }}</span>
      </Button>
    </PopoverTrigger>
    <PopoverContent align="start" class="w-[290px] space-y-4" :aria-label="label">
      <div class="flex items-center justify-between gap-3">
        <Label :for="`${id}-color`">{{ copy.color }}</Label>
        <input :id="`${id}-color`" type="color" :value="opaqueHex" class="h-9 w-16 cursor-pointer rounded-md border border-input bg-transparent p-1" :disabled="disabled" @input="updateColor">
      </div>
      <div class="space-y-2">
        <div class="flex items-center justify-between gap-3 text-sm">
          <Label :for="`${id}-alpha`">{{ copy.opacity }}</Label>
          <output :for="`${id}-alpha`" class="tabular-nums text-muted-foreground">{{ opacity }}%</output>
        </div>
        <Slider :id="`${id}-alpha`" :min="0" :max="100" :step="1" :model-value="[opacity]" class="py-1.5" :disabled="disabled" :aria-label="copy.opacity" @update:model-value="updateAlpha" />
      </div>
      <div class="space-y-2">
        <Label :for="`${id}-value`">{{ copy.format }}</Label>
        <Input :id="`${id}-value`" v-model="draft" class="font-mono text-xs" :disabled="disabled" :aria-invalid="invalid" :aria-describedby="invalid ? `${id}-error` : undefined" spellcheck="false" @blur="commitDraft" @keydown.enter.prevent="commitDraft" />
        <p v-if="invalid" :id="`${id}-error`" class="text-xs text-destructive" role="alert">
          {{ copy.invalid }}
        </p>
      </div>
      <div v-if="swatches.length" class="space-y-2">
        <p class="text-sm text-muted-foreground">
          {{ copy.presets }}
        </p>
        <div class="flex flex-wrap gap-2" role="group" :aria-label="copy.presets">
          <Button v-for="swatch in swatches" :key="swatch" type="button" variant="outline" size="icon" class="color-checker size-7 overflow-hidden border-border p-0" :disabled="disabled" :aria-label="swatch" :aria-pressed="serializeColor(parseColor(swatch)!) === selectedHex" @click="chooseSwatch(swatch)">
            <span class="block size-full" :style="{ backgroundColor: swatch }" />
          </Button>
        </div>
      </div>
      <Button type="button" class="w-full" :disabled="disabled" @click="confirmColor">
        确定
      </Button>
    </PopoverContent>
  </Popover>
</template>

<style scoped>
.color-checker {
  background-color: #fff;
  background-image: linear-gradient(45deg, #a1a1aa 25%, transparent 25%), linear-gradient(-45deg, #a1a1aa 25%, transparent 25%), linear-gradient(45deg, transparent 75%, #a1a1aa 75%), linear-gradient(-45deg, transparent 75%, #a1a1aa 75%);
  background-size: 10px 10px;
  background-position: 0 0, 0 5px, 5px -5px, -5px 0;
}
</style>
