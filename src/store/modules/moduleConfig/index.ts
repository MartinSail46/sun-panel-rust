import { toast } from '@/lib/feedback'
import { defineStore } from 'pinia'
import { getValueByName, save } from '@/api/system/moduleConfig'
import { createConfirmedWriter } from '@/utils/confirmedWrite'
import { isUncertainWrite } from '@/utils/requestLifetime'
import type { Response } from '@/utils/request'

interface ModuleSlot { revision?: number; tail: Promise<unknown>; writer?: ReturnType<typeof createConfirmedWriter<unknown>>; response?: Response<unknown> }
const slots = new WeakMap<object, Map<string, ModuleSlot>>()
function slot(owner: object, name: string) {
  let map = slots.get(owner)
  if (!map) { map = new Map(); slots.set(owner, map) }
  let entry = map.get(name)
  if (!entry) { entry = { tail: Promise.resolve() }; map.set(name, entry) }
  return entry
}

export const useModuleConfig = defineStore('module-config-store', {
  actions: {
    async getValueByNameFromCloud<T>(name: string) {
      const response = await getValueByName<T>(`module-${name}`)
      if (response.code !== 0)
        throw response
      slot(this, name).revision = response.revision ?? 0
      return response
    },
    saveToCloud<T = unknown>(name: string, value: unknown) {
      const entry = slot(this, name)
      const snapshot = JSON.parse(JSON.stringify(value))
      const work = entry.tail.catch(() => {}).then(async () => {
        if (entry.revision === undefined)
          await this.getValueByNameFromCloud(name)
        entry.writer ??= createConfirmedWriter<unknown>({
          async read() {
            const response = await getValueByName<unknown>(`module-${name}`, { silent: true })
            if (response.code !== 0) throw response
            return { value: response.data, revision: response.revision ?? 0 }
          },
          async write(value, revision) {
            const response = await save<unknown>(`module-${name}`, value, revision, { silent: true })
            if (response.code !== 0) throw response
            entry.response = response
            return response.revision ?? revision + 1
          },
        })
        entry.response = undefined
        let confirmedRevision: number
        try { confirmedRevision = await entry.writer(snapshot, entry.revision ?? 0) }
        catch (error) {
          if (error && typeof error === 'object' && 'code' in error && error.code === 1409) {
            toast.error('其他页面修改了同一设置，请载入服务器配置后重新编辑。')
            throw new Error('其他页面修改了同一设置，请载入服务器配置后重新编辑。')
          }
          if (isUncertainWrite(error)) error.message = '操作结果尚未确认，请先刷新检查，避免重复提交。'
          throw error
        }
        const response = (entry.response || { code: 0, msg: 'OK', data: snapshot, revision: confirmedRevision }) as Response<T>
        if (response.code !== 0)
          throw response
        entry.revision = response.revision ?? entry.revision
        return response
      })
      entry.tail = work
      return work
    },
  },
})
