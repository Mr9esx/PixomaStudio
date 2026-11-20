import { Cross2Icon } from '@radix-ui/react-icons'
import type { Table } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { DataTableViewOptions } from './view-options'

export function DataTableToolbar<TData>({ table, searchPlaceholder, searchKey }: { table: Table<TData>; searchPlaceholder?: string; searchKey?: string }) {
  const { t } = useTranslation()
  const placeholder = searchPlaceholder ?? t('dataTable.search')
  const isFiltered = table.getState().columnFilters.length > 0 || table.getState().globalFilter
  return <div className='-ms-1 -me-1 flex items-center justify-between gap-2'>
    <div className='flex flex-1 flex-col-reverse items-start gap-2 sm:flex-row sm:items-center'>
      <Input aria-label={placeholder} placeholder={placeholder} className='h-8 w-37.5 lg:w-62.5'
        value={searchKey ? (table.getColumn(searchKey)?.getFilterValue() as string) ?? '' : table.getState().globalFilter ?? ''}
        onChange={(event) => searchKey ? table.getColumn(searchKey)?.setFilterValue(event.target.value) : table.setGlobalFilter(event.target.value)} />
      {isFiltered ? <Button variant='ghost' size='sm' onClick={() => { table.resetColumnFilters(); table.setGlobalFilter('') }}>{t('dataTable.reset')}<Cross2Icon /></Button> : null}
    </div>
    <DataTableViewOptions table={table} />
  </div>
}
