import { ss } from '@/utils/storage'

const LOCAL_NAME = 'appSetting'

export type Theme = 'light' | 'dark' | 'auto'

export interface AppState {
  theme: Theme
}

export function defaultSetting(): AppState {
  return { theme: 'auto' }
}

export function getLocalSetting(): AppState {
  const localSetting: AppState | undefined = ss.get(LOCAL_NAME)
  const defaults = defaultSetting()
  return { theme: localSetting?.theme ?? defaults.theme }
}

export function setLocalSetting(setting: AppState): void {
  ss.set(LOCAL_NAME, setting)
}
