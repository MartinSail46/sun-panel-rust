import { isUncertainWrite } from './requestLifetime.ts'

export interface NavigationMoveItem { id?: number; itemIconGroupId?: number; sort?: number }
export interface NavigationMoveGroup { id?: number; items: NavigationMoveItem[] }
export interface NavigationMove { itemId: number; targetGroupId: number; targetIndex: number }
export interface NavigationMoveRequest extends NavigationMove { revision: number }
export interface NavigationSnapshot<G extends NavigationMoveGroup> { groups: G[]; revision: number }
export interface NavigationMoveState { saving: boolean; needsVerification: boolean }
export type NavigationMoveResult = 'saved' | 'verified' | 'reloaded'

export function cloneNavigation<G extends NavigationMoveGroup>(groups: readonly G[]): G[] {
  return groups.map(group => ({ ...group, items: group.items.map(item => ({ ...item })) }))
}

/** targetIndex refers to the destination list after removing the moved item. */
export function applyNavigationMove<G extends NavigationMoveGroup>(groups: readonly G[], move: NavigationMove): G[] {
  if (!Number.isSafeInteger(move.itemId) || move.itemId <= 0 || !Number.isSafeInteger(move.targetGroupId) || move.targetGroupId <= 0)
    throw new RangeError('请选择有效的应用和目标分组')
  const next = cloneNavigation(groups)
  const matches = next.flatMap(group => group.items.filter(item => item.id === move.itemId))
  if (matches.length !== 1)
    throw new RangeError('待移动的应用不存在或数据重复，请刷新后重试')
  const targets = next.filter(group => group.id === move.targetGroupId)
  if (targets.length !== 1)
    throw new RangeError('目标分组不存在或数据重复，请刷新后重试')
  const target = targets[0]!
  const source = next.find(group => group.items.some(item => item.id === move.itemId))!
  for (const group of next)
    group.items = group.items.filter(item => item.id !== move.itemId)
  if (!Number.isSafeInteger(move.targetIndex) || move.targetIndex < 0 || move.targetIndex > target.items.length)
    throw new RangeError('目标位置无效，请刷新后重试')
  const item = { ...matches[0]!, itemIconGroupId: move.targetGroupId }
  target.items.splice(move.targetIndex, 0, item)
  for (const group of new Set([source, target]))
    group.items = group.items.map((entry, sort) => ({ ...entry, sort }))
  return next
}

function sameLayout(left: readonly NavigationMoveGroup[], right: readonly NavigationMoveGroup[]): boolean {
  return left.length === right.length && left.every((group, index) => {
    const other = right[index]
    return other && group.id === other.id && group.items.length === other.items.length
      && group.items.every((item, itemIndex) => item.id === other.items[itemIndex]?.id && item.itemIconGroupId === other.items[itemIndex]?.itemIconGroupId)
  })
}

interface NavigationMoverOptions<G extends NavigationMoveGroup> {
  read: () => Promise<NavigationSnapshot<G>>
  write: (move: NavigationMoveRequest) => Promise<number>
  apply: (snapshot: NavigationSnapshot<G>) => void
  onStateChange?: (state: NavigationMoveState) => void
}

/** One atomic move at a time. An uncertain write is verified using reads, never replayed. */
export function createNavigationMover<G extends NavigationMoveGroup>(options: NavigationMoverOptions<G>) {
  let state: NavigationMoveState = { saving: false, needsVerification: false }
  let awaiting: { desired: NavigationSnapshot<G> } | undefined
  const setState = (next: NavigationMoveState) => {
    state = next
    options.onStateChange?.({ ...state })
  }

  async function verifyMove(): Promise<NavigationMoveResult> {
    if (state.saving)
      throw Object.assign(new Error('正在保存应用移动，请稍候'), { code: 'NAVIGATION_BUSY' })
    if (!awaiting)
      return 'reloaded'
    setState({ saving: true, needsVerification: true })
    try {
      const remote = await options.read()
      const result = sameLayout(awaiting.desired.groups, remote.groups) ? 'verified' : 'reloaded'
      options.apply(remote)
      awaiting = undefined
      setState({ saving: false, needsVerification: false })
      return result
    }
    catch (error) {
      setState({ saving: false, needsVerification: true })
      throw error
    }
  }

  async function move(snapshot: NavigationSnapshot<G>, intent: NavigationMove): Promise<NavigationMoveResult> {
    if (state.saving || state.needsVerification)
      throw Object.assign(new Error(state.saving ? '正在保存应用移动，请稍候' : '请先确认上次移动的结果，再移动其他应用'), { code: state.saving ? 'NAVIGATION_BUSY' : 'NAVIGATION_UNVERIFIED' })
    if (!Number.isSafeInteger(snapshot.revision) || snapshot.revision < 0)
      throw new RangeError('导航数据版本无效，请刷新后重试')
    const desired = { groups: applyNavigationMove(snapshot.groups, intent), revision: snapshot.revision }
    if (sameLayout(snapshot.groups, desired.groups))
      return 'saved'
    setState({ saving: true, needsVerification: false })
    options.apply(desired)
    try {
      const revision = await options.write({ ...intent, revision: snapshot.revision })
      options.apply({ ...desired, revision })
      setState({ saving: false, needsVerification: false })
      return 'saved'
    }
    catch (error) {
      // A read may fail even for a rejected request; leave the board blocked until
      // it can be reconciled instead of restoring a snapshot that may be stale.
      awaiting = { desired }
      let remote: NavigationSnapshot<G>
      try { remote = await options.read() }
      catch {
        setState({ saving: false, needsVerification: true })
        throw error
      }
      const matches = sameLayout(desired.groups, remote.groups)
      if (isUncertainWrite(error) && !matches && remote.revision <= snapshot.revision) {
        // The server may still commit after the client deadline. Keep the
        // optimistic draft until the user explicitly requests a fresh read.
        setState({ saving: false, needsVerification: true })
        throw error
      }
      options.apply(remote)
      awaiting = undefined
      setState({ saving: false, needsVerification: false })
      if (matches && isUncertainWrite(error))
        return 'verified'
      throw error
    }
  }

  return { move, verifyMove, get state() { return { ...state } } }
}
