/** Integer RGBA bytes keep hexadecimal alpha stable when colors are reopened. */
export interface RgbaColor {
  r: number
  g: number
  b: number
  a: number
}

function byte(value: string, percentageScale = 255): number | null {
  const token = value.trim()
  if (!/^(?:\d+(?:\.\d*)?|\.\d+)%?$/.test(token))
    return null
  const percent = token.endsWith('%')
  const number = Number(percent ? token.slice(0, -1) : token)
  const maximum = percent ? 100 : percentageScale
  if (!Number.isFinite(number) || number < 0 || number > maximum)
    return null
  return Math.round(number / maximum * 255)
}

/** Parse only formats supported by the editor; invalid input never becomes black. */
export function parseColor(value: string | null | undefined): RgbaColor | null {
  if (typeof value !== 'string')
    return null
  const input = value.trim().toLowerCase()
  if (input === 'transparent')
    return { r: 0, g: 0, b: 0, a: 0 }
  const hexadecimal = /^#([\da-f]{3}|[\da-f]{4}|[\da-f]{6}|[\da-f]{8})$/.exec(input)
  if (hexadecimal) {
    const raw = hexadecimal[1]
    const full = raw.length <= 4 ? [...raw].map(part => part + part).join('') : raw
    return {
      r: Number.parseInt(full.slice(0, 2), 16),
      g: Number.parseInt(full.slice(2, 4), 16),
      b: Number.parseInt(full.slice(4, 6), 16),
      a: full.length === 8 ? Number.parseInt(full.slice(6, 8), 16) : 255,
    }
  }
  const functional = /^(rgb|rgba)\((.*)\)$/.exec(input)
  if (!functional)
    return null
  const body = functional[2].trim()
  let channels: string[]
  let alpha: string | undefined
  if (body.includes(',')) {
    if (body.includes('/'))
      return null
    const parts = body.split(',').map(part => part.trim())
    if (parts.length !== 3 && parts.length !== 4)
      return null
    channels = parts.slice(0, 3)
    alpha = parts[3]
    if (functional[1] === 'rgba' && alpha === undefined)
      return null
  }
  else {
    const parts = body.split('/')
    if (parts.length > 2)
      return null
    channels = parts[0].trim().split(/\s+/)
    alpha = parts[1]?.trim()
    if (channels.length !== 3 || (parts.length === 2 && !alpha))
      return null
  }
  const values = channels.map(channel => byte(channel))
  const a = alpha === undefined ? 255 : byte(alpha, 1)
  if (values.includes(null) || a === null)
    return null
  return { r: values[0]!, g: values[1]!, b: values[2]!, a }
}

export function serializeColor(color: RgbaColor, includeAlpha = color.a !== 255): string {
  const values = [color.r, color.g, color.b, color.a]
  if (!values.every(value => Number.isInteger(value) && value >= 0 && value <= 255))
    throw new TypeError('Color channels must be integer bytes')
  return `#${values.slice(0, includeAlpha ? 4 : 3).map(value => value.toString(16).padStart(2, '0')).join('')}`
}
