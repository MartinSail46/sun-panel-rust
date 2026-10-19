import assert from 'node:assert/strict'
import { Buffer } from 'node:buffer'
import { execFileSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { readFileSync } from 'node:fs'
import process from 'node:process'
import { setTimeout as delay } from 'node:timers/promises'

const version = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version
const image = process.env.SUN_PANEL_TEST_IMAGE || `sun-panel-rust:${version}`
const name = `sun-panel-smoke-${randomUUID()}`
const docker = (...args) => execFileSync('docker', args, { encoding: 'utf8' }).trim()
let started = false
try {
  docker('run', '--detach', '--rm', '--name', name, '--read-only', '--tmpfs', '/tmp:size=16m,noexec,nosuid,nodev', '--cap-drop=ALL', '--security-opt', 'no-new-privileges:true', '--publish', '127.0.0.1::3002', image)
  started = true
  const containerBase = () => `http://${docker('port', name, '3002/tcp').split('\n')[0]}`
  let base = containerBase()
  const waitReady = async () => {
    for (let attempt = 0; attempt < 40; attempt++) {
      try {
        const response = await fetch(`${base}/api/health`)
        if (response.ok) {
          assert.equal((await response.json()).data.version, version, 'backend reports the release version')
          return
        }
      }
      catch { /* the process is still starting */ }
      await delay(250)
    }
    assert.fail('container should become ready with the expected version')
  }
  await waitReady()
  const call = async (path, body = {}) => {
    const response = await fetch(base + path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) })
    assert.equal(response.status, 200, path)
    const value = await response.json()
    return value
  }
  const postEnvelope = async (path, body = {}) => {
    const value = await call(path, body)
    assert.equal(value.code, 0, `${path}: ${JSON.stringify(value)}`)
    return value
  }
  const post = async (path, body = {}) => (await postEnvelope(path, body)).data
  const page = await fetch(base)
  assert.equal(page.headers.get('cache-control'), 'no-cache')
  const html = await page.text()
  const entry = html.match(/src="(\/assets\/[^" ]+\.js)"/)?.[1]
  assert.ok(entry, 'built entry should be present')
  const plain = await fetch(base + entry, { headers: { 'accept-encoding': 'identity' } })
  const plainText = await plain.text()
  assert.match(plain.headers.get('cache-control'), /immutable/)
  for (const encoding of ['gzip', 'br']) {
    const response = await fetch(base + entry, { headers: { 'accept-encoding': encoding } })
    assert.equal(response.headers.get('content-encoding'), encoding)
    assert.match(response.headers.get('vary'), /accept-encoding/i)
    assert.equal(await response.text(), plainText)
    assert.ok(Number(response.headers.get('content-length')) < Buffer.byteLength(plainText))
  }
  const conditional = await fetch(base + entry, { headers: { 'accept-encoding': 'identity', 'if-none-match': plain.headers.get('etag') } })
  assert.equal(conditional.status, 304)
  for (const path of [
    '/login', '/user/getInfo', '/user/getAuthInfo', '/notice/getListByDisplayType',
    '/openness/getDisclaimer', '/openness/getAboutDescription', '/file/uploadFiles',
    '/system/monitor/getAll', '/system/monitor/getCpuState', '/system/monitor/getMemonyState',
    '/system/monitor/getDiskStateByPath', '/panel/itemIcon/addMultiple', '/panel/itemIcon/getListByGroupId',
    '/panel/itemIcon/saveSort',
  ]) {
    assert.equal((await fetch(`${base}/api${path}`, { method: 'POST' })).status, 404, path)
  }
  const rejectedLegacy = await call('/api/panel/navigation/import/preview', { appName: 'Sun-Panel-Config', version: 1, icons: [] })
  assert.equal(rejectedLegacy.code, 1400, 'legacy JSON imports are no longer supported')
  const emptySnapshot = await post('/api/panel/navigation/export')
  assert.deepEqual(emptySnapshot.groups, [], 'a new workspace has a valid empty snapshot')
  const emptyPreview = await post('/api/panel/navigation/import/preview', emptySnapshot)
  await post('/api/panel/navigation/import', { ...emptySnapshot, revision: emptyPreview.revision })
  assert.deepEqual((await post('/api/panel/navigation/export')).groups, [], 'an empty snapshot round-trips')
  const group = await post('/api/panel/itemIconGroup/edit', { title: 'Container regression', icon: 'mdi:folder', description: 'Container navigation metadata', sort: 0 })
  const first = await post('/api/panel/itemIcon/edit', { title: 'First', url: 'http://192.168.1.10/first', openMethod: 2, itemIconGroupId: group.id, sort: 0 })
  const second = await post('/api/panel/itemIcon/edit', { title: 'Second', url: 'http://192.168.1.10/second', openMethod: 2, itemIconGroupId: group.id, sort: 1 })
  const beforeMove = await postEnvelope('/api/panel/navigation')
  const sameGroupMove = await postEnvelope('/api/panel/navigation/move', { itemId: second.id, targetGroupId: group.id, targetIndex: 0, revision: beforeMove.revision })
  assert.equal(sameGroupMove.revision, sameGroupMove.data.revision)
  assert.ok(sameGroupMove.revision > beforeMove.revision)
  const navigation = await post('/api/panel/navigation')
  assert.deepEqual(navigation.list[0].items.map(item => item.title), ['Second', 'First'])
  assert.ok(navigation.list.every(group => group.items.every(item => !Object.hasOwn(item, 'lanUrl'))), 'navigation emits one URL')
  const staleMove = await call('/api/panel/navigation/move', { itemId: first.id, targetGroupId: group.id, targetIndex: 0, revision: beforeMove.revision })
  assert.equal(staleMove.code, 1409, 'another page cannot move against a stale navigation snapshot')
  assert.equal(staleMove.currentRevision, sameGroupMove.revision)
  const targetGroup = await post('/api/panel/itemIconGroup/edit', { title: 'Move target', sort: 1 })
  const emptyGroup = await post('/api/panel/itemIconGroup/edit', { title: 'Empty target', sort: 2 })
  const left = await post('/api/panel/itemIcon/edit', { title: 'Left', url: 'http://192.168.1.10/left', openMethod: 2, itemIconGroupId: targetGroup.id, sort: 0 })
  const right = await post('/api/panel/itemIcon/edit', { title: 'Right', url: 'http://192.168.1.10/right', openMethod: 2, itemIconGroupId: targetGroup.id, sort: 1 })
  const targetItems = snapshot => snapshot.data.list.find(current => current.id === targetGroup.id).items
  let moveSnapshot = await postEnvelope('/api/panel/navigation')
  await post('/api/panel/navigation/move', { itemId: first.id, targetGroupId: targetGroup.id, targetIndex: 1, revision: moveSnapshot.revision })
  moveSnapshot = await postEnvelope('/api/panel/navigation')
  assert.deepEqual(targetItems(moveSnapshot).map(item => item.id), [left.id, first.id, right.id], 'a cross-group move can insert between existing items')
  assert.deepEqual(targetItems(moveSnapshot).map(item => item.sort), [0, 1, 2])
  assert.ok(targetItems(moveSnapshot).every(item => item.itemIconGroupId === targetGroup.id))
  await post('/api/panel/navigation/move', { itemId: first.id, targetGroupId: emptyGroup.id, targetIndex: 0, revision: moveSnapshot.revision })
  moveSnapshot = await postEnvelope('/api/panel/navigation')
  assert.deepEqual(moveSnapshot.data.list.find(current => current.id === emptyGroup.id).items.map(item => item.id), [first.id], 'an empty group is a valid drop target')
  assert.deepEqual(targetItems(moveSnapshot).map(item => item.id), [left.id, right.id])
  const invalidMove = await call('/api/panel/navigation/move', { itemId: first.id, targetGroupId: group.id, targetIndex: 2, revision: moveSnapshot.revision })
  assert.equal(invalidMove.code, 1400, 'out-of-range positions cannot partially update either group')
  assert.deepEqual(await postEnvelope('/api/panel/navigation'), moveSnapshot)
  await post('/api/panel/navigation/move', { itemId: first.id, targetGroupId: group.id, targetIndex: 1, revision: moveSnapshot.revision })

  const initialConfig = await postEnvelope('/api/panel/userConfig/get')
  assert.equal(initialConfig.revision, initialConfig.data.revision)
  const savedConfig = await postEnvelope('/api/panel/userConfig/set', { revision: initialConfig.revision, panel: { logoText: 'Saved configuration' } })
  assert.equal(savedConfig.revision, initialConfig.revision + 1)
  const staleConfig = await call('/api/panel/userConfig/set', { revision: initialConfig.revision, panel: { logoText: 'Stale overwrite' } })
  assert.equal(staleConfig.code, 1409)
  assert.equal(staleConfig.currentRevision, savedConfig.revision)
  const missingConfigRevision = await call('/api/panel/userConfig/set', { panel: { logoText: 'Unversioned overwrite' } })
  assert.equal(missingConfigRevision.code, 1409)
  const config = await post('/api/panel/userConfig/get')
  assert.equal(config.panel.logoText, 'Saved configuration', 'stale clients cannot overwrite newer values')

  const initialModule = await postEnvelope('/api/system/moduleConfig/getByName', { name: 'clock' })
  assert.equal(initialModule.revision, 0)
  const savedModule = await postEnvelope('/api/system/moduleConfig/save', { revision: initialModule.revision, name: 'clock', value: { label: 'Saved module' } })
  assert.equal(savedModule.revision, 1)
  const staleModule = await call('/api/system/moduleConfig/save', { revision: initialModule.revision, name: 'clock', value: { label: 'Stale module' } })
  assert.equal(staleModule.code, 1409)
  assert.equal(staleModule.currentRevision, savedModule.revision)
  const module = await postEnvelope('/api/system/moduleConfig/getByName', { name: 'clock' })
  assert.equal(module.data.label, 'Saved module')
  assert.equal(module.revision, savedModule.revision)
  assert.ok(!Object.hasOwn(module.data, 'revision'), 'module data keeps its original shape')

  const document = {
    appName: 'Sun-Panel-Navigation',
    schemaVersion: 1,
    mode: 'merge',
    groups: [{
      title: 'Reviewed import',
      icon: 'mdi:nas',
      description: 'Preserved import metadata',
      sort: 0,
      items: [{ title: 'Imported NAS', url: 'http://192.168.1.11:5000', openMethod: 2, sort: 0 }],
    }],
  }
  for (let attempt = 0; attempt < 2; attempt++) {
    const preview = await post('/api/panel/navigation/import/preview', document)
    assert.deepEqual(preview.conflicts, [])
    assert.equal(preview.newItems, attempt === 0 ? 1 : 0)
    assert.equal(preview.duplicateItems, attempt === 0 ? 0 : 1)
    const imported = await post('/api/panel/navigation/import', { ...document, revision: preview.revision })
    assert.equal(imported.newItems, attempt === 0 ? 1 : 0)
  }
  const reviewed = await post('/api/panel/navigation/import/preview', document)
  await post('/api/panel/itemIconGroup/edit', { ...group, description: 'Changed after import preview' })
  const staleImport = await call('/api/panel/navigation/import', { ...document, revision: reviewed.revision })
  assert.equal(staleImport.code, 1409, 'navigation changes invalidate a reviewed import')
  const exported = await post('/api/panel/navigation/export')
  const importedGroup = exported.groups.find(group => group.title === 'Reviewed import')
  assert.ok(importedGroup)
  assert.equal(importedGroup.icon, 'mdi:nas')
  assert.equal(importedGroup.description, 'Preserved import metadata')
  assert.equal(importedGroup.sort, 0)
  assert.equal(importedGroup.items.length, 1, 'repeated merge must not duplicate items')
  assert.equal(importedGroup.items[0].url, 'http://192.168.1.11:5000', 'the saved single URL round-trips')
  assert.equal(importedGroup.items[0].sort, 0)
  assert.ok(exported.groups.every(group => group.items.every(item => !Object.hasOwn(item, 'lanUrl'))), 'native export emits one URL')
  const imageBytes = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScLbtAAAAABJRU5ErkJggg==', 'base64')
  const form = new FormData()
  form.append('imgfile', new Blob([imageBytes], { type: 'image/png' }), 'check.png')
  const uploaded = await (await fetch(`${base}/api/file/uploadImg`, { method: 'POST', body: form })).json()
  assert.equal(uploaded.code, 0)
  const bytes = Buffer.from(await (await fetch(base + uploaded.data.imageUrl)).arrayBuffer())
  assert.deepEqual(bytes, imageBytes)
  const managedImage = await post('/api/file/getList')
  const file = managedImage.list.find(file => file.src === uploaded.data.imageUrl)
  assert.ok(file, 'uploaded file appears in manager')
  const withWallpaper = await postEnvelope('/api/panel/userConfig/set', {
    revision: config.revision,
    panel: { ...config.panel, backgroundImageSrc: uploaded.data.imageUrl },
  })
  await post('/api/panel/itemIcon/edit', { ...first, sort: 1, icon: { itemType: 2, src: uploaded.data.imageUrl } })
  const referencedFiles = await post('/api/file/getList')
  const referenced = referencedFiles.list.find(current => current.id === file.id)
  assert.ok(referenced.referenceCount >= 2)
  assert.ok(referenced.references.some(reference => reference.type === 'wallpaper'))
  assert.ok(referenced.references.some(reference => reference.type === 'navigation'))
  const blocked = await call('/api/file/deletes', { ids: [file.id] })
  assert.equal(blocked.code, 1310, 'in-use wallpaper and icons cannot be deleted')
  assert.ok(blocked.data.files.some(file => file.id === referenced.id))
  const clearedWallpaper = await postEnvelope('/api/panel/userConfig/set', {
    revision: withWallpaper.revision,
    panel: { ...config.panel, backgroundImageSrc: '' },
  })
  assert.equal((await call('/api/file/deletes', { ids: [file.id] })).code, 1310, 'remaining navigation reference still protects the image')
  await post('/api/panel/itemIcon/edit', { ...first, sort: 1, icon: null })
  await post('/api/file/deletes', { ids: [file.id] })
  assert.equal((await fetch(base + uploaded.data.imageUrl)).status, 404)
  const retiredReference = await call('/api/panel/userConfig/set', {
    revision: clearedWallpaper.revision,
    panel: { ...config.panel, backgroundImageSrc: uploaded.data.imageUrl },
  })
  assert.equal(retiredReference.code, 1311, 'a deleted managed image cannot be written back')
  const snapshot = await post('/api/system/monitor/snapshot', { diskPaths: [] })
  assert.ok(snapshot.cpuInfo && snapshot.memoryInfo && snapshot.disks)
  const beforeDelete = await postEnvelope('/api/panel/navigation')
  const deleted = await postEnvelope('/api/panel/itemIcon/deletes', { ids: [first.id], revision: beforeDelete.revision })
  const deletedRef = deleted.data.trash.find(entry => entry.kind === 'item' && entry.id === first.id)
  assert.ok(deletedRef?.deletionId, 'deletion returns its persistent trash identity')
  docker('restart', '--timeout', '5', name)
  base = containerBase()
  await waitReady()
  const afterRestart = await postEnvelope('/api/panel/navigation')
  assert.equal(afterRestart.revision, deleted.revision, 'navigation revision survives a process restart')
  assert.ok(afterRestart.data.list.every(group => group.items.every(item => item.id !== first.id)))
  assert.ok((await post('/api/panel/navigation/trash/list')).list.some(entry => entry.id === first.id && entry.deletionId === deletedRef.deletionId), 'trash survives a process restart')
  const restored = await postEnvelope('/api/panel/navigation/trash/restore', { ...deletedRef, revision: afterRestart.revision })
  assert.ok((await post('/api/panel/navigation')).list.some(group => group.items.some(item => item.id === first.id)), 'the original item is restored after restart')
  const deletedAgain = await postEnvelope('/api/panel/itemIcon/deletes', { ids: [first.id], revision: restored.revision })
  await post('/api/panel/navigation/trash/purge', { ...deletedAgain.data.trash[0], revision: deletedAgain.revision })
  assert.ok((await post('/api/panel/navigation/trash/list')).list.every(entry => entry.id !== first.id || entry.kind !== 'item'))
  assert.equal((await post('/api/panel/userConfig/get')).panel.logoText, 'Saved configuration', 'configuration survives a process restart')
  docker('exec', name, '/app/sun-panel-rust', '--healthcheck')
  console.log('Container regression passed: release version, cross-group moves, revision conflicts, config/modules, import/export, image references, restart persistence, trash restore/purge, monitor and compressed assets.')
}
catch (error) {
  if (started)
    console.error(docker('logs', name))
  throw error
}
finally {
  if (started)
    docker('stop', '--timeout', '5', name)
}
