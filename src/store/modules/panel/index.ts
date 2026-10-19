import { toast } from '@/lib/feedback'
import { defineStore } from 'pinia'
import { defaultStatePanelConfig, getLocalState, normalizePanelConfig as normalize, setLocalState } from './helper'
import { get as getUserConfig, set as setUserConfig } from '@/api/panel/userConfig'
import { createPanelSync } from '@/utils/panelSync'
import { createConfirmedWriter } from '@/utils/confirmedWrite'
import { isUncertainWrite } from '@/utils/requestLifetime'
import type { SaveState } from '@/utils/settingsSave'

interface PanelOwner {
  panelConfig: Panel.panelConfig
  recordState: () => void
}
const controllers = new WeakMap<PanelOwner, ReturnType<typeof createPanelSync<Panel.panelConfig>>>()
const revisionSignal = 'sun-panel-config-revision'


function controller(owner: PanelOwner) {
  let sync = controllers.get(owner)
  if (!sync) {
    const read = async () => {
      const response = await getUserConfig<Panel.userConfig & { revision: number }>({ silent: true })
      if (response.code !== 0) throw response
      return { value: normalize(response.data.panel), revision: response.data.revision ?? response.revision ?? 0 }
    }
    const write = createConfirmedWriter<Panel.panelConfig>({
      read,
      async write(value, revision) {
        const response = await setUserConfig<{ revision: number }>({ panel: value, revision }, { silent: true })
        if (response.code !== 0) throw response
        return response.revision ?? response.data.revision
      },
    })
    sync = createPanelSync(owner.panelConfig, {
      conflictMessage: () => '其他页面修改了同一设置，请载入服务器配置后重新编辑。',
      read,
      async write(value, revision) {
        try { return await write(value, revision) }
        catch (error) {
          if (isUncertainWrite(error)) error.message = '操作结果尚未确认，请先刷新检查，避免重复提交。'
          throw error
        }
      },
      apply(value) {
        owner.panelConfig = normalize(value)
        owner.recordState()
      },
      published(revision) {
        try { localStorage.setItem(revisionSignal, JSON.stringify({ revision, at: Date.now() })) }
        catch { /* A notification failure cannot undo a committed save. */ }
      },
    })
    controllers.set(owner, sync)
    const current = sync
    current.subscribe((state) => {
      if (state.status === 'error')
        toast.error(state.error || '配置保存失败，请重试')
    })
    window.addEventListener('storage', (event) => {
      if (event.key === revisionSignal)
        void current.refreshIfClean().catch(() => {})
    })
  }
  return sync
}

export const usePanelState = defineStore('panel', {
  state: (): Panel.State => getLocalState(),
  actions: {
    loadPanelConfig() { return controller(this).load() },
    schedulePanelSave(snapshot: Panel.panelConfig) {
      controller(this).observe(normalize(snapshot))
      this.recordState()
    },
    subscribePanelSave(listener: (state: SaveState) => void) {
      return controller(this).subscribe(listener)
    },
    flushPanelSave() { return controller(this).flush() },
    async savePanelChanges(patch: Partial<Panel.panelConfig>) {
      // Observe before modifying state, including writes made before Style was opened.
      const sync = controller(this)
      this.panelConfig = normalize({ ...this.panelConfig, ...patch })
      sync.observe(this.panelConfig)
      this.recordState()
      return sync.flush()
    },
    async resetPanelConfig() {
      const sync = controller(this)
      this.panelConfig = defaultStatePanelConfig()
      sync.observe(this.panelConfig)
      this.recordState()
      return sync.flush()
    },
    async reloadPanelConfig() {
      try { return await controller(this).reload() }
      catch { return false }
    },
    recordState() {
      try { setLocalState(this.$state) }
      catch { /* The server remains authoritative if browser storage is unavailable. */ }
    },
  },
})
