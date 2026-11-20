import { useMemo } from 'react'
import { type ColumnDef, getCoreRowModel, getFilteredRowModel, getPaginationRowModel, getSortedRowModel, useReactTable } from '@tanstack/react-table'
import { MoreHorizontal, Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import type { WorkflowVersion } from '@/lib/api/workflows'
import { formatDateTime } from '@/lib/format'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuGroup, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Empty, EmptyHeader, EmptyTitle } from '@/components/ui/empty'
import { DataTableColumnHeader } from '@/components/data-table/column-header'
import { DataTable } from '@/components/data-table/data-table'
import { DataTableToolbar } from '@/components/data-table/toolbar'

export type WorkflowVersionAction = 'view' | 'test' | 'default' | 'restore'

export function WorkflowVersionList({ versions, defaultVersionId, busy = false, onAction, onCreate }: {
  versions: WorkflowVersion[]
  defaultVersionId?: string | null
  busy?: boolean
  onAction: (action: WorkflowVersionAction, version: WorkflowVersion) => void
  onCreate: () => void
}) {
  const { t } = useTranslation()
  const columns = useMemo<ColumnDef<WorkflowVersion>[]>(() => [
    { accessorKey: 'version', enableHiding: false,
      header: ({ column }) => <DataTableColumnHeader column={column} title={t('workflowMarket.version')} />,
      cell: ({ row }) => <div className='flex max-w-48 flex-wrap items-center gap-1.5'>
        <Button size='sm' variant='link' className='min-w-0 px-0 tabular-nums' disabled={busy} onClick={() => onAction('view', row.original)}><span className='max-w-40 truncate'>{row.original.version}</span></Button>
        {row.original.id === defaultVersionId ? <Badge variant='secondary'>{t('workflowMarket.defaultVersion')}</Badge> : null}
      </div>,
    },
    { accessorKey: 'notes', enableSorting: false, meta: { label: t('workflowMarket.versionNotes') },
      header: ({ column }) => <DataTableColumnHeader column={column} title={t('workflowMarket.versionNotes')} />,
      cell: ({ row }) => <p className='max-w-64 whitespace-normal break-all text-muted-foreground'>{row.original.notes || '—'}</p>,
    },
    { accessorKey: 'created_at', meta: { label: t('workflowMarket.createdAt') },
      header: ({ column }) => <div data-version-metadata><DataTableColumnHeader column={column} title={t('workflowMarket.createdAt')} className='justify-end' /></div>,
      cell: ({ row }) => <div data-version-metadata className='text-right tabular-nums'>{formatDateTime(row.original.created_at)}</div>,
    },
    { id: 'actions', enableHiding: false,
      header: ({ column }) => <DataTableColumnHeader column={column} title={t('common.actions')} className='text-right' />,
      cell: ({ row }) => <div className='flex justify-end'><DropdownMenu>
        <DropdownMenuTrigger asChild><Button size='icon-sm' variant='ghost' aria-label={t('common.moreActions')} disabled={busy}><MoreHorizontal /></Button></DropdownMenuTrigger>
        <DropdownMenuContent align='end'><DropdownMenuGroup>
          <DropdownMenuItem onSelect={() => onAction('view', row.original)}>{t('workflowMarket.versionContent')}</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => onAction('test', row.original)}>{t('workflowMarket.testVersion')}</DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem disabled={row.original.id === defaultVersionId} onSelect={() => onAction('default', row.original)}>{t('workflowMarket.setDefault')}</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => onAction('restore', row.original)}>{t('workflowMarket.restoreDraft')}</DropdownMenuItem>
        </DropdownMenuGroup></DropdownMenuContent>
      </DropdownMenu></div>,
    },
  ], [t, defaultVersionId, busy, onAction])
  const table = useReactTable({ data: versions, columns, getRowId: (version) => version.id,
    getCoreRowModel: getCoreRowModel(), getFilteredRowModel: getFilteredRowModel(),
    getSortedRowModel: getSortedRowModel(), getPaginationRowModel: getPaginationRowModel(),
    initialState: { sorting: [{ id: 'created_at', desc: true }], pagination: { pageIndex: 0, pageSize: 10 } },
  })
  return <div className='@container/content min-w-0'><DataTable table={table}
    className='[&_tr>:has([data-version-metadata])]:hidden @min-[640px]/content:[&_tr>:has([data-version-metadata])]:table-cell'
    emptyState={<Empty className='py-8'><EmptyHeader><EmptyTitle>{t(versions.length ? 'dataTable.noResults' : 'workflowMarket.noVersions')}</EmptyTitle></EmptyHeader></Empty>}>
    <div className='flex flex-wrap items-center gap-2'>
      <div className='min-w-0 flex-1'><DataTableToolbar table={table} searchPlaceholder={t('workflowMarket.searchVersions')} /></div>
      <Button size='sm' disabled={busy} onClick={onCreate}><Plus />{t('workflowMarket.createVersion')}</Button>
    </div>
  </DataTable></div>
}
