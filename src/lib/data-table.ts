import type { Column } from '@tanstack/react-table'

export function getColumnPinningStyle<TData>({ column }: { column: Column<TData> }): React.CSSProperties {
  const pinned = column.getIsPinned()
  return {
    left: pinned === 'left' ? `${column.getStart('left')}px` : undefined,
    right: pinned === 'right' ? `${column.getAfter('right')}px` : undefined,
    position: pinned ? 'sticky' : 'relative',
    background: pinned ? 'var(--card)' : undefined,
    width: column.getSize(),
    zIndex: pinned ? 1 : undefined,
  }
}
