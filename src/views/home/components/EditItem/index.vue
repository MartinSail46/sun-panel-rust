<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import IconEditor from './IconEditor.vue'
import { edit, getSiteFavicon } from '@/api/panel/itemIcon'
import { getList as getGroupList } from '@/api/panel/itemIconGroup'
import { RoundCardModal } from '@/components/common'
import AppSelect from '@/components/common/AppSelect.vue'
import ColorPicker from '@/components/common/ColorPicker.vue'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Switch } from '@/components/ui/switch'
import { toast } from '@/lib/feedback'
import { cloneDraft } from '@/store/modules/navigation/helpers'
import { useNavigationStore } from '@/store/modules/navigation'
import { isRequestCancelled, isUncertainWrite } from '@/utils/requestLifetime'

interface Props {
  visible: boolean
  itemInfo: Panel.ItemInfo | null
  itemGroupId?: number
}

const props = defineProps<Props>()
const emit = defineEmits<{
  (e: 'update:visible', visible: boolean): void
  (e: 'done', item: Panel.ItemInfo): void
}>()
const submitLoading = ref(false)
const getIconLoading = ref(false)
const navigation = useNavigationStore()
const draftVersion = ref(0)
let draftRequests: AbortController | undefined
const itemIconGroupOptions = ref<{ label: string, value: number }[]>([])
const restoreDefault: Panel.ItemInfo = {
  icon: null,
  title: '',
  url: '',
  description: '',
  openMethod: 2,
}
function createItemDraft(item: Panel.ItemInfo | null): Panel.ItemInfo & { icon: Panel.ItemIcon } {
  const draft = cloneDraft(item || restoreDefault)
  return {
    ...draft,
    icon: {
      itemType: 2,
      ...draft.icon,
      style: draft.icon?.style ?? 1,
      textColor: draft.icon?.textColor || '#ffffff',
      hideDescription: draft.icon?.hideDescription ?? false,
      hideTitle: draft.icon?.hideTitle ?? false,
    },
  }
}
const model = ref(createItemDraft(props.itemInfo))
const iconStyleOptions = [{ label: '小图标', value: 1 }, { label: '详情图标', value: 0 }]
const formRef = ref<HTMLFormElement | null>(null)
const errors = ref<Partial<Record<'title' | 'url', string>>>({})
const options = computed(() => [
  { label: '当前页面打开', value: 1 },
  { label: '新窗口打开', value: 2 },
  { label: '当前页面弹窗打开', value: 3 },
])
const show = computed({
  get: () => props.visible,
  set: (visible: boolean) => emit('update:visible', visible),
})

function setIconStyle(value: string | number) {
  if (value === 0 || value === 1)
    model.value.icon.style = value
}

function clearError(field: 'title' | 'url') {
  delete errors.value[field]
}

async function handleSubmit() {
  if (submitLoading.value)
    return
  errors.value = {}
  for (const field of ['title', 'url'] as const) {
    if (!model.value[field].trim())
      errors.value[field] = '必填项'
  }
  if (Object.keys(errors.value).length) {
    await nextTick()
    formRef.value?.querySelector<HTMLInputElement>('[aria-invalid="true"]')?.focus()
    return
  }
  const version = draftVersion.value
  const payload = cloneDraft(model.value)
  submitLoading.value = true
  try {
    const { code, data, msg } = await navigation.withMutation(() => edit<Panel.ItemInfo>(payload))
    if (code === 0) {
      // A completed request must not close a subsequently opened draft.
      if (version === draftVersion.value) {
        show.value = false
        model.value = createItemDraft(null)
        emit('done', data)
      }
      // A subsequent read failure cannot turn an acknowledged save into failure.
      await navigation.refresh().catch(() => {})
    }
    else {
      toast.error(`${'保存失败'}:${msg}`)
    }
  }
  catch (error) {
    if (isUncertainWrite(error)) {
      // Reconcile with a read only. A timed-out create must never be replayed.
      await navigation.refresh().catch(() => {})
      toast.warning('操作结果尚未确认，请先刷新检查，避免重复提交。')
    }
    else if (!isRequestCancelled(error)) {
      toast.error('保存失败')
    }
  }
  finally {
    submitLoading.value = false
  }
}

async function getIconByUrl(url: string) {
  if (getIconLoading.value || submitLoading.value)
    return
  getIconLoading.value = true
  const version = draftVersion.value
  try {
    const { code, data } = await getSiteFavicon<{ iconUrl: string }>(url, { signal: draftRequests?.signal })
    if (version !== draftVersion.value)
      return
    if (code === 0)
      model.value.icon = { ...model.value.icon, itemType: 2, src: data.iconUrl }
    else
      toast.error('图标获取失败')
  }
  catch (error) {
    if (!isRequestCancelled(error) && version === draftVersion.value)
      toast.error('图标获取失败')
  }
  finally {
    if (version === draftVersion.value)
      getIconLoading.value = false
  }
}

watch(() => props.visible, (visible) => {
  draftVersion.value++
  draftRequests?.abort()
  draftRequests = visible ? new AbortController() : undefined
  getIconLoading.value = false
  errors.value = {}
  if (visible) {
    model.value = createItemDraft(props.itemInfo)
    if (props.itemGroupId)
      model.value.itemIconGroupId = props.itemGroupId
    void getGroupListOptions()
  }
}, { immediate: true })

async function getGroupListOptions() {
  const version = draftVersion.value
  try {
    const { data, code, msg } = await getGroupList<Common.ListResponse<Panel.ItemIconGroup[]>>({ signal: draftRequests?.signal })
    if (version !== draftVersion.value)
      return
    if (code === 0) {
      itemIconGroupOptions.value = data.list.map(group => ({ value: group.id as number, label: group.title as string }))
      if (!model.value.itemIconGroupId)
        model.value.itemIconGroupId = data.list[0]?.id
    }
    else {
      toast.error(`${'分组信息获取失败'}:${msg}`)
    }
  }
  catch (error) {
    if (!isRequestCancelled(error) && version === draftVersion.value)
      toast.error('分组信息获取失败')
  }
}
onBeforeUnmount(() => { draftRequests?.abort() })
</script>

<template>
  <RoundCardModal v-model:show="show" class="sm:max-w-[600px]" :mask-closable="!submitLoading" :close-on-esc="!submitLoading" :closable="!submitLoading" :title="itemInfo ? '修改项目' : '添加项目'">
    <form id="item-editor-form" ref="formRef" class="space-y-4" novalidate :aria-busy="submitLoading" @submit.prevent="handleSubmit">
      <fieldset class="min-w-0 space-y-4" :disabled="submitLoading">
        <div class="grid gap-4 sm:grid-cols-2">
          <div class="space-y-2">
            <Label for="item-group">分组</Label>
            <AppSelect id="item-group" v-model="model.itemIconGroupId" :options="itemIconGroupOptions" :disabled="submitLoading" aria-label="分组" />
          </div>
          <div class="space-y-2">
            <Label for="item-title">标题</Label>
            <Input id="item-title" v-model="model.title" :maxlength="20" required :aria-invalid="!!errors.title" :aria-describedby="errors.title ? 'item-title-error' : undefined" @update:model-value="clearError('title')" />
            <p v-if="errors.title" id="item-title-error" class="text-sm text-destructive" role="alert">
              {{ errors.title }}
            </p>
          </div>
        </div>
        <div class="space-y-2">
          <Label>图标</Label>
          <IconEditor v-if="visible" :key="draftVersion" v-model:item-icon="model.icon" :disabled="submitLoading" />
          <div class="grid gap-3 pt-1 sm:grid-cols-2">
            <div class="space-y-2">
              <Label for="item-icon-style">图标风格</Label>
              <AppSelect id="item-icon-style" :model-value="model.icon.style" :options="iconStyleOptions" :disabled="submitLoading" aria-label="图标风格" @update:model-value="setIconStyle" />
            </div>
            <div class="space-y-2">
              <Label for="item-text-color">文字颜色</Label>
              <ColorPicker id="item-text-color" v-model="model.icon.textColor" class="w-full" :swatches="['#000000', '#ffffff', '#18A058', '#2080F0', '#F0A020']" :disabled="submitLoading" aria-label="文字颜色" />
            </div>
          </div>
          <div v-if="model.icon.style === 0" class="flex min-h-11 items-center justify-between gap-4 text-sm">
            <Label for="item-hide-description">隐藏描述</Label>
            <Switch id="item-hide-description" v-model="model.icon.hideDescription" :disabled="submitLoading" />
          </div>
          <div v-else class="flex min-h-11 items-center justify-between gap-4 text-sm">
            <Label for="item-hide-title">隐藏标题</Label>
            <Switch id="item-hide-title" v-model="model.icon.hideTitle" :disabled="submitLoading" />
          </div>
        </div>
        <div class="space-y-2">
          <Label for="item-url">地址</Label>
          <div class="flex flex-wrap gap-2 sm:flex-nowrap">
            <Input id="item-url" v-model="model.url" class="min-w-0 flex-1" :maxlength="1000" placeholder="http(s)://" required :aria-invalid="!!errors.url" :aria-describedby="errors.url ? 'item-url-error' : undefined" @update:model-value="clearError('url')" />
            <Button type="button" variant="outline" class="shrink-0" :disabled="!model.url || getIconLoading || submitLoading" :aria-busy="getIconLoading" @click="getIconByUrl(model.url)">
              <span v-if="getIconLoading" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
              获取图标
            </Button>
          </div>
          <p v-if="errors.url" id="item-url-error" class="text-sm text-destructive" role="alert">
            {{ errors.url }}
          </p>
        </div>
        <div class="space-y-2">
          <Label for="item-description">描述信息</Label>
          <Textarea id="item-description" v-model="model.description" :maxlength="100" rows="2" />
        </div>
        <div class="space-y-2">
          <Label for="item-open-method">打开方式</Label>
          <AppSelect id="item-open-method" v-model="model.openMethod" :options="options" :disabled="submitLoading" aria-label="打开方式" />
        </div>
      </fieldset>
    </form>
    <template #footer>
      <div class="flex justify-end gap-2">
        <Button type="button" variant="outline" :disabled="submitLoading" @click="show = false">
          取消
        </Button>
        <Button type="submit" form="item-editor-form" :disabled="submitLoading" :aria-busy="submitLoading">
          <span v-if="submitLoading" class="size-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true" />
          保存
        </Button>
      </div>
    </template>
  </RoundCardModal>
</template>
