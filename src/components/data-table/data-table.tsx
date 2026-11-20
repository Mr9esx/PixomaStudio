import { flexRender, type Table as TanstackTable } from '@tanstack/react-table'
import { getColumnPinningStyle } from '@/lib/data-table'
import { cn } from '@/lib/utils'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { DataTablePagination } from './pagination'

type DataTableProps<TData> = React.ComponentProps<'div'> & {
  table: TanstackTable<TData>
  actionBar?: React.ReactNode
  emptyState?: React.ReactNode
  onRowClick?: (original: TData) => void
  selectedRowId?: string
  getRowId?: (original: TData) => string
  hidePagination?: boolean
  surface?: 'page' | 'card'
}

export function DataTable<TData>({ table, actionBar, emptyState, onRowClick, selectedRowId,
  getRowId, hidePagination = false, surface = 'page', className, children, ...props }: DataTableProps<TData>) {
  const rows = table.getRowModel().rows
  return <div className={cn('flex w-full flex-col gap-3 overflow-auto', className)} {...props}>
    {children ? <div className='px-1 first:pt-1'>{children}</div> : null}
    <div className={cn('overflow-hidden rounded-lg border', surface === 'card' ? 'bg-card' : 'bg-background')}>
      <Table>
        <TableHeader>{table.getHeaderGroups().map((group) => <TableRow key={group.id} className='bg-muted/40'>
          {group.headers.map((header) => <TableHead key={header.id} colSpan={header.colSpan} className='p-0' style={getColumnPinningStyle({ column: header.column })}>
            {header.isPlaceholder ? null : flexRender(header.column.columnDef.header, header.getContext())}
          </TableHead>)}
        </TableRow>)}</TableHeader>
        <TableBody>{rows.length ? rows.map((row) => {
          const selected = selectedRowId != null && getRowId?.(row.original) === selectedRowId
          return <TableRow key={row.id} data-state={selected && 'selected'}
            className={cn(onRowClick && 'cursor-pointer', selected && 'bg-muted/50')}
            onClick={onRowClick ? () => onRowClick(row.original) : undefined}>
            {row.getVisibleCells().map((cell) => <TableCell key={cell.id} style={getColumnPinningStyle({ column: cell.column })}>
              {flexRender(cell.column.columnDef.cell, cell.getContext())}
            </TableCell>)}
          </TableRow>
        }) : <TableRow><TableCell colSpan={table.getVisibleLeafColumns().length} className='h-24 p-0 text-center'>{emptyState}</TableCell></TableRow>}</TableBody>
      </Table>
    </div>
    {hidePagination ? null : <DataTablePagination table={table} />}
    {actionBar && table.getFilteredSelectedRowModel().rows.length > 0 && actionBar}
  </div>
}
