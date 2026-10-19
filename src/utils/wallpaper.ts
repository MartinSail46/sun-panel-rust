// A stable public URL remains valid when a saved configuration outlives a build.
export const DEFAULT_WALLPAPER = '/default-wallpaper.webp'

export function normalizeWallpaperSource(source?: string): string {
  return source?.trim() || DEFAULT_WALLPAPER
}
