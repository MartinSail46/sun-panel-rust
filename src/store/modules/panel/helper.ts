import { ss } from '@/utils/storage'
import { DEFAULT_WALLPAPER, normalizeWallpaperSource } from '@/utils/wallpaper'
const LOCAL_NAME = 'panelStorage'

export function defaultStatePanelConfig(): Panel.panelConfig {
  return {
    backgroundImageSrc: DEFAULT_WALLPAPER,
    backgroundBlur: 0,
    backgroundMaskNumber: 0,
    logoText: 'Sun-Panel',
    clockShowSecond: false,
    searchBoxShow: false,
    marginBottom: 10,
    marginTop: 10,
    maxWidth: 1200,
    maxWidthUnit: 'px',
    marginX: 5,
    footerHtml: '',
    systemMonitorShow: false,
    systemMonitorShowTitle: true,

  }
}

export function normalizePanelConfig(value?: Partial<Panel.panelConfig>): Panel.panelConfig {
  const defaults = defaultStatePanelConfig()
  return {
    ...Object.fromEntries(Object.entries(defaults).map(([key, fallback]) => [key, value?.[key as keyof Panel.panelConfig] ?? fallback])),
    backgroundImageSrc: normalizeWallpaperSource(value?.backgroundImageSrc),
  } as Panel.panelConfig
}

export function getLocalState(): Panel.State {
  const localState = ss.get(LOCAL_NAME) as Partial<Panel.State> | null
  return { panelConfig: normalizePanelConfig(localState?.panelConfig) }
}

export function setLocalState(state: Panel.State) {
  ss.set(LOCAL_NAME, state)
}
