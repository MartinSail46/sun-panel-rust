// Only repository-owned SVGs enter this sprite; icon names remain compatible with saved panels.
const icons = import.meta.glob<string>('../assets/svg-icons/*.svg', {
  query: '?raw',
  import: 'default',
  eager: true,
})

export function registerSvgIcons() {
  const namespace = 'http://www.w3.org/2000/svg'
  const sprite = document.createElementNS(namespace, 'svg')
  sprite.id = 'sun-panel-svg-sprite'
  sprite.setAttribute('aria-hidden', 'true')
  sprite.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden;pointer-events:none'

  for (const [path, source] of Object.entries(icons)) {
    const svg = new DOMParser().parseFromString(source, 'image/svg+xml').documentElement
    const symbol = document.createElementNS(namespace, 'symbol')
    for (const attribute of Array.from(svg.attributes)) {
      if (!['xmlns', 'width', 'height', 'id'].includes(attribute.name))
        symbol.setAttribute(attribute.name, attribute.value)
    }
    symbol.id = path.split('/').pop()!.replace(/\.svg$/, '')
    for (const child of Array.from(svg.childNodes))
      symbol.appendChild(document.importNode(child, true))
    sprite.appendChild(symbol)
  }

  document.getElementById(sprite.id)?.remove()
  document.body.prepend(sprite)
}
