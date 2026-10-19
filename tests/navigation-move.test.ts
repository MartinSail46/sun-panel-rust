import assert from 'node:assert/strict'
import test from 'node:test'
import { createServer } from 'node:http'
import { once } from 'node:events'
import { applyNavigationMove, createNavigationMover } from '../src/utils/navigationMove.ts'
import { RequestError, runRequestLifecycle } from '../src/utils/requestLifetime.ts'
import type { NavigationMoveGroup, NavigationSnapshot } from '../src/utils/navigationMove.ts'

function board() {
  return [
    { id: 10, title: 'Apps', description: 'keep group metadata', items: [
      { id: 1, itemIconGroupId: 10, title: 'One', sort: 0, icon: { text: 'O' } },
      { id: 2, itemIconGroupId: 10, title: 'Two', sort: 1, icon: { text: 'T' } },
      { id: 3, itemIconGroupId: 10, title: 'Three', sort: 2, icon: { text: '3' } },
    ] },
    { id: 20, title: 'Tools', description: '', items: [{ id: 4, itemIconGroupId: 20, title: 'Four', sort: 0, icon: { text: 'F' } }] },
    { id: 30, title: 'Empty', description: '', items: [] },
  ]
}
const intent = { itemId: 2, targetGroupId: 20, targetIndex: 0 }
const ids = (groups: NavigationMoveGroup[]) => groups.map(group => group.items.map(item => item.id))

test('cross-group insertion preserves every item, metadata and source without mutating it', () => {
  const source = board()
  const moved = applyNavigationMove(source, intent)
  assert.deepEqual(ids(moved), [[1, 3], [2, 4], []])
  assert.deepEqual(ids(source), [[1, 2, 3], [4], []])
  assert.equal(moved[0]!.description, source[0]!.description)
  assert.deepEqual(moved[1]!.items[0], { ...source[0]!.items[1], itemIconGroupId: 20, sort: 0 })
  assert.deepEqual(moved.flatMap(group => group.items.map(item => item.id)).sort(), [1, 2, 3, 4])
})

test('same-group positions use indices after removal, including first and last', () => {
  assert.deepEqual(ids(applyNavigationMove(board(), { itemId: 1, targetGroupId: 10, targetIndex: 2 })), [[2, 3, 1], [4], []])
  assert.deepEqual(ids(applyNavigationMove(board(), { itemId: 3, targetGroupId: 10, targetIndex: 0 })), [[3, 1, 2], [4], []])
  assert.deepEqual(ids(applyNavigationMove(board(), { itemId: 2, targetGroupId: 10, targetIndex: 1 })), [[1, 2, 3], [4], []])
})

test('empty groups accept an item and existing groups accept insertion at the end', () => {
  const moved = applyNavigationMove(board(), { itemId: 2, targetGroupId: 30, targetIndex: 0 })
  assert.deepEqual(ids(moved), [[1, 3], [4], [2]])
  assert.equal(moved[2]!.items[0]!.itemIconGroupId, 30)
  assert.deepEqual(moved.map(group => group.items.map(item => item.sort)), [[0, 1], [0], [0]])
  assert.deepEqual(ids(applyNavigationMove(board(), { ...intent, targetIndex: 1 })), [[1, 3], [4, 2], []])
})

test('invalid, duplicate and unreachable targets fail without changing the source', () => {
  const source = board()
  for (const invalid of [
    { ...intent, itemId: 999 }, { ...intent, targetGroupId: 999 },
    { ...intent, targetIndex: -1 }, { ...intent, targetIndex: 2 },
    { ...intent, targetIndex: 0.5 }, { ...intent, itemId: 0 },
  ]) assert.throws(() => applyNavigationMove(source, invalid), RangeError)
  assert.throws(() => applyNavigationMove([...source, source[0]!], intent), /数据重复/)
  assert.deepEqual(ids(source), [[1, 2, 3], [4], []])
})

test('optimistic move sends one revision-checked write and confirms its revision', async () => {
  const before = { groups: board(), revision: 6 }
  const seen: NavigationSnapshot<ReturnType<typeof board>[number]>[] = []
  const requests: unknown[] = []
  const mover = createNavigationMover({
    read: async () => { throw new Error('must not read after confirmed success') },
    write: async (request) => { requests.push(request); assert.deepEqual(ids(seen[0]!.groups), [[1, 3], [2, 4], []]); return 7 },
    apply: value => seen.push(value),
  })
  assert.equal(await mover.move(before, intent), 'saved')
  assert.deepEqual(requests, [{ ...intent, revision: 6 }])
  assert.equal(seen.at(-1)!.revision, 7)
  assert.deepEqual(mover.state, { saving: false, needsVerification: false })
})

test('no-op moves send no write and overlapping saves are refused', async () => {
  let finish!: (revision: number) => void
  let writes = 0
  const mover = createNavigationMover({
    read: async () => ({ groups: board(), revision: 1 }),
    write: () => { writes++; return new Promise<number>((resolve) => { finish = resolve }) },
    apply: () => {},
  })
  assert.equal(await mover.move({ groups: board(), revision: 1 }, { itemId: 2, targetGroupId: 10, targetIndex: 1 }), 'saved')
  assert.equal(writes, 0)
  const saving = mover.move({ groups: board(), revision: 1 }, intent)
  assert.equal(mover.state.saving, true)
  await assert.rejects(mover.move({ groups: board(), revision: 1 }, intent), { code: 'NAVIGATION_BUSY' })
  finish(2)
  await saving
  assert.equal(writes, 1)
})

test('a timed-out committed move is read back as saved without replaying the write', async () => {
  const desired = applyNavigationMove(board(), intent)
  let writes = 0
  let current = { groups: board(), revision: 6 }
  const mover = createNavigationMover({
    read: async () => ({ groups: desired, revision: 7 }),
    write: async () => { writes++; throw new RequestError('timeout', 'write') },
    apply: value => { current = value },
  })
  assert.equal(await mover.move({ groups: board(), revision: 6 }, intent), 'verified')
  assert.equal(writes, 1)
  assert.equal(current.revision, 7)
  assert.deepEqual(ids(current.groups), [[1, 3], [2, 4], []])
})

test('revision conflict applies the new server board, preserving the conflict and never overwriting it', async () => {
  const conflict = { code: 1409, currentRevision: 8, msg: 'changed' }
  const remote = { groups: applyNavigationMove(board(), { itemId: 3, targetGroupId: 30, targetIndex: 0 }), revision: 8 }
  let current = { groups: board(), revision: 6 }
  let writes = 0
  const mover = createNavigationMover({
    read: async () => remote,
    write: async () => { writes++; throw conflict },
    apply: value => { current = value },
  })
  await assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), error => error === conflict)
  assert.equal(writes, 1)
  assert.deepEqual(current, remote)
  assert.deepEqual(mover.state, { saving: false, needsVerification: false })
})

test('unknown write plus failed read retains the optimistic board and blocks further writes until read-only verification', async () => {
  const failure = new RequestError('network', 'write')
  let reads = 0
  let writes = 0
  let current = { groups: board(), revision: 6 }
  const mover = createNavigationMover({
    read: async () => { if (++reads < 3) throw new Error('offline'); return { groups: applyNavigationMove(board(), intent), revision: 7 } },
    write: async () => { writes++; throw failure },
    apply: value => { current = value },
  })
  await assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), error => error === failure)
  assert.deepEqual(ids(current.groups), [[1, 3], [2, 4], []])
  assert.deepEqual(mover.state, { saving: false, needsVerification: true })
  await assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), { code: 'NAVIGATION_UNVERIFIED' })
  await assert.rejects(mover.verifyMove(), /offline/)
  assert.equal(mover.state.needsVerification, true)
  assert.equal(await mover.verifyMove(), 'verified')
  assert.equal(current.revision, 7)
  assert.equal(writes, 1)
  assert.deepEqual(mover.state, { saving: false, needsVerification: false })
})

test('verification of an uncommitted move reloads actual data rather than retrying the write', async () => {
  let reads = 0
  let writes = 0
  let current = { groups: board(), revision: 6 }
  const mover = createNavigationMover({
    read: async () => { if (++reads === 1) throw new Error('offline'); return { groups: board(), revision: 6 } },
    write: async () => { writes++; throw new RequestError('timeout', 'write') },
    apply: value => { current = value },
  })
  await assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), { outcomeUnknown: true })
  assert.equal(await mover.verifyMove(), 'reloaded')
  assert.deepEqual(ids(current.groups), [[1, 2, 3], [4], []])
  assert.equal(writes, 1)
})


test('an old revision read after timeout stays blocked until explicit verification of a late HTTP commit', async (context) => {
  let remote = { groups: board(), revision: 6 }
  let writes = 0
  let reads = 0
  let releaseCommit!: () => void
  let didCommit!: () => void
  let receivedWrite!: () => void
  const held = new Promise<void>((resolve) => { releaseCommit = resolve })
  const committed = new Promise<void>((resolve) => { didCommit = resolve })
  const arrived = new Promise<void>((resolve) => { receivedWrite = resolve })
  const server = createServer(async (request, response) => {
    if (request.method === 'POST') {
      writes++
      receivedWrite()
      await held
      remote = { groups: applyNavigationMove(board(), intent), revision: 7 }
      didCommit()
    }
    else {
      reads++
    }
    response.setHeader('Content-Type', 'application/json')
    response.end(JSON.stringify(remote))
  })
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  const address = server.address()
  assert.ok(address && typeof address !== 'string')
  const url = `http://127.0.0.1:${address.port}`
  let current = { groups: board(), revision: 6 }
  const mover = createNavigationMover({
    read: async () => (await fetch(url)).json() as Promise<typeof remote>,
    write: () => runRequestLifecycle(async (signal) => {
      const response = await fetch(url, { method: 'POST', signal })
      return (await response.json() as typeof remote).revision
    }, { operation: 'write', timeoutMs: 100 }),
    apply: value => { current = value },
  })
  try {
    // Advance the deadline only after the server has accepted the write. A real
    // 100 ms timer could otherwise abort connection setup on a busy test runner.
    context.mock.timers.enable({ apis: ['setTimeout'] })
    const timedOut = assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), { kind: 'timeout', outcomeUnknown: true })
    await arrived
    context.mock.timers.tick(100)
    await timedOut
    context.mock.timers.reset()
    assert.equal(writes, 1)
    assert.equal(reads, 1)
    assert.deepEqual(ids(current.groups), [[1, 3], [2, 4], []])
    assert.deepEqual(mover.state, { saving: false, needsVerification: true })
    await assert.rejects(mover.move({ groups: board(), revision: 6 }, intent), { code: 'NAVIGATION_UNVERIFIED' })
    releaseCommit()
    await committed
    assert.equal(await mover.verifyMove(), 'verified')
    assert.equal(current.revision, 7)
    assert.equal(writes, 1)
    assert.equal(reads, 2)
  }
  finally {
    context.mock.timers.reset()
    releaseCommit()
    server.closeAllConnections()
    await new Promise<void>((resolve) => { server.close(() => resolve()) })
  }
})
