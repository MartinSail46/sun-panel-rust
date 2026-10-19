export interface HistoryTrashRef { kind: 'item' | 'group'; id: number; deletionId?: string }
export type NavigationUndoAction =
  | { kind: 'move'; itemId: number; targetGroupId: number; targetIndex: number; title: string }
  | { kind: 'delete'; entry: HistoryTrashRef; title: string; position?: { groupId: number; index: number } }
  | { kind: 'sort'; order: { id: number; sort: number }[] }

export interface NavigationUndoRecord { action: NavigationUndoAction; revision: number }

/** Only this session's acknowledged changes can be undone. External edits invalidate history. */
export function createNavigationHistory(onChange: (record: NavigationUndoRecord | undefined, count: number) => void, limit = 20) {
  const records: NavigationUndoRecord[] = []
  const publish = () => onChange(records.at(-1), records.length)
  function clear() {
    records.length = 0
    publish()
  }
  return {
    record(action: NavigationUndoAction, revision: number) {
      records.push({ action: structuredClone(action), revision })
      if (records.length > limit) records.shift()
      publish()
    },
    reconcile(revision: number) {
      if (records.length && records.at(-1)!.revision !== revision) clear()
    },
    completeUndo(revision: number) {
      records.pop()
      // Our own undo changes the revision, while returning to the previous layout.
      if (records.length) records.at(-1)!.revision = revision
      publish()
    },
    clear,
  }
}
